-- Sole database source of truth: complete destructive reset and current install.
-- Running this file removes every existing Stellarion game, player, turn,
-- event, policy, and RPC before recreating the current database contract.
-- Run the entire file in the Supabase SQL Editor as postgres to install or reset.
-- Enable anonymous sign-ins in Supabase Auth settings for the game client.
-- This file is the entire application backend setup. The game calls these SQL
-- RPCs directly through PostgREST using its authenticated user token. There is
-- no Edge Function, Docker, service-role client key, or separate server deployment.
-- Rebuild/restart the client after changing its RPC contract so every client
-- uses the exact database contract defined here.
-- The shared Rust model on clients generates maps and resolves turns. PostgreSQL
-- validates payload structure, membership, lifecycle, revisions, and submission
-- completeness, but does not independently execute the Rust simulation. This is
-- a client-simulated game, not an anti-cheat server for modified clients.
-- For local verification: use Node.js 24 and Rust, then run just verify-sql.
-- Test tooling is installed under ignored target/sql-verification/.
-- This generates current Rust test snapshots and executes disposable PostgreSQL
-- with pg_cron registration stubbed. It never resets the hosted project.

begin;

-- Supabase system schemas (auth, storage, extensions, and Realtime) stay intact;
-- the complete application-facing database is reset in one operation.
drop schema if exists public cascade;
create schema public;
grant usage on schema public to postgres, anon, authenticated, service_role;
grant all on schema public to postgres;

create schema if not exists extensions;
-- Supabase runs custom privilege hooks for CREATE EXTENSION, even when IF NOT
-- EXISTS skips installation. Avoid invoking those hooks again for an installed
-- extension: their revokes can conflict with existing dependent privileges.
-- Supabase supplies the scheduler permissions; do not change its managed grants.
do $$
begin
    if not exists (select 1 from pg_catalog.pg_extension where extname = 'pgcrypto') then
        create extension pgcrypto with schema extensions;
    end if;
    if not exists (select 1 from pg_catalog.pg_extension where extname = 'pg_cron') then
        create extension pg_cron with schema pg_catalog;
    end if;
end;
$$;

-- Jobs live outside public, so replace every Stellarion job on each reset while
-- preserving unrelated schedules.
delete from cron.job_run_details
 where jobid in (
     select jobid from cron.job
      where database = current_database() and jobname like 'stellarion-%'
 );
select cron.unschedule(jobid)
  from cron.job
 where database = current_database() and jobname like 'stellarion-%';

-- Authenticated clients must never be able to create shadow objects used by
-- SECURITY DEFINER functions.
revoke create on schema public from public;

create table public.stellarion_games (
    id uuid primary key default gen_random_uuid(),
    code text not null unique,
    created_by uuid not null references auth.users(id) on delete restrict,
    max_players smallint not null,
    status text not null,
    state jsonb not null,
    revision bigint not null default 0,
    current_turn bigint not null,
    event_sequence bigint not null default 0,
    created_at timestamptz not null default clock_timestamp(),
    -- Snapshot save time is intentionally separate from updated_at: presence and
    -- durable notification traffic must not make a stale save look recent.
    saved_at timestamptz not null default clock_timestamp(),
    updated_at timestamptz not null default clock_timestamp(),
    finished_at timestamptz,
    constraint stellarion_games_code_format check (code ~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{6}$'),
    constraint stellarion_games_player_count check (max_players between 2 and 4),
    constraint stellarion_games_status check (status in ('lobby', 'active', 'finished')),
    constraint stellarion_games_revision check (revision >= 0),
    constraint stellarion_games_turn check (current_turn >= 1),
    constraint stellarion_games_event_sequence check (event_sequence >= 0),
    constraint stellarion_games_state_object check (jsonb_typeof(state) = 'object')
);

-- Completion time is independent of later saves, recovery, and presence events.
create function public.stellarion_stamp_finished_game()
returns trigger
language plpgsql
set search_path = pg_catalog, public
as $$
begin
    if new.status = 'finished' and old.status is distinct from new.status then
        new.finished_at := clock_timestamp();
    end if;
    return new;
end;
$$;

create trigger stellarion_game_finished_at
    before update of status on public.stellarion_games
    for each row execute function public.stellarion_stamp_finished_game();

create index stellarion_finished_games_expiry
    on public.stellarion_games (finished_at)
    where status = 'finished';

-- All games, including lobbies, expire 30 days after their last snapshot save.
-- Presence, recovery, and notification traffic do not extend this deadline.
create index stellarion_saved_games_expiry
    on public.stellarion_games (saved_at);

create table public.stellarion_game_players (
    game_id uuid not null references public.stellarion_games(id) on delete cascade,
    player_id bigint not null,
    user_id uuid not null references auth.users(id) on delete restrict,
    display_name text not null,
    recovery_code text not null,
    is_creator boolean not null default false,
    identity_version bigint not null default 1,
    connected boolean not null default false,
    joined_at timestamptz not null default clock_timestamp(),
    last_seen_at timestamptz not null default clock_timestamp(),
    primary key (game_id, player_id),
    constraint stellarion_game_players_user unique (game_id, user_id),
    constraint stellarion_game_players_recovery unique (game_id, recovery_code),
    constraint stellarion_game_players_slot check (player_id between 1 and 4),
    -- Player names are canonicalized by the RPCs and capped to match the client text field.
    constraint stellarion_game_players_name check (
        char_length(btrim(display_name)) between 1 and 16
        and display_name = btrim(display_name)
    ),
    constraint stellarion_game_players_recovery_code check (
        recovery_code ~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{4}(-[0-9ABCDEFGHJKMNPQRSTVWXYZ]{4}){3}$'
    ),
    constraint stellarion_game_players_identity_version check (identity_version >= 1)
);

create unique index stellarion_one_creator_per_game
    on public.stellarion_game_players (game_id)
    where is_creator;

create index stellarion_game_players_user_lookup
    on public.stellarion_game_players (user_id, game_id);

create table public.stellarion_turn_submissions (
    game_id uuid not null,
    turn bigint not null,
    player_id bigint not null,
    submission jsonb not null,
    digest text not null,
    ready boolean not null default true,
    submitted_at timestamptz not null default clock_timestamp(),
    primary key (game_id, turn, player_id),
    constraint stellarion_turn_submissions_member
        foreign key (game_id, player_id)
        references public.stellarion_game_players(game_id, player_id)
        on delete cascade,
    constraint stellarion_turn_submissions_turn check (turn >= 1),
    constraint stellarion_turn_submissions_payload check (jsonb_typeof(submission) = 'object'),
    constraint stellarion_turn_submissions_digest check (digest ~ '^[0-9a-f]{64}$')
);

create index stellarion_turn_submissions_resolution
    on public.stellarion_turn_submissions (game_id, turn, player_id);

-- Joint attacks are private coordination records. They deliberately contain only the selected
-- target, objective, and volunteered fleets; no caller uploads or replaces the game snapshot.
create table public.stellarion_joint_attacks (
    game_id uuid not null references public.stellarion_games(id) on delete cascade,
    attack_id bigint not null,
    turn bigint not null,
    inviter bigint not null,
    invitation jsonb not null,
    created_at timestamptz not null default clock_timestamp(),
    updated_at timestamptz not null default clock_timestamp(),
    primary key (game_id, attack_id),
    constraint stellarion_joint_attacks_member
        foreign key (game_id, inviter)
        references public.stellarion_game_players(game_id, player_id)
        on delete cascade,
    constraint stellarion_joint_attacks_id check (attack_id > 0),
    constraint stellarion_joint_attacks_turn check (turn >= 1),
    constraint stellarion_joint_attacks_payload check (jsonb_typeof(invitation) = 'object')
);

create index stellarion_joint_attacks_turn
    on public.stellarion_joint_attacks (game_id, turn, attack_id);

-- Trading Post negotiations are private, current-turn coordination records. A finalized row is
-- also represented inside the canonical Rust snapshot, where deterministic resolution reserves
-- each outgoing bundle immediately and delivers both incoming bundles at the turn boundary.
create table public.stellarion_trades (
    game_id uuid not null references public.stellarion_games(id) on delete cascade,
    trade_id bigint not null,
    turn bigint not null,
    player_low bigint not null,
    player_high bigint not null,
    invitation jsonb not null,
    created_at timestamptz not null default clock_timestamp(),
    updated_at timestamptz not null default clock_timestamp(),
    primary key (game_id, trade_id),
    constraint stellarion_trades_low_member foreign key (game_id, player_low)
        references public.stellarion_game_players(game_id, player_id) on delete cascade,
    constraint stellarion_trades_high_member foreign key (game_id, player_high)
        references public.stellarion_game_players(game_id, player_id) on delete cascade,
    constraint stellarion_trades_id check (trade_id > 0),
    constraint stellarion_trades_turn check (turn >= 1),
    constraint stellarion_trades_pair check (player_low < player_high),
    constraint stellarion_trades_payload check (jsonb_typeof(invitation) = 'object'),
    constraint stellarion_one_trade_per_pair_turn unique (game_id, turn, player_low, player_high)
);

create index stellarion_trades_turn
    on public.stellarion_trades (game_id, turn, trade_id);

create table public.stellarion_game_events (
    game_id uuid not null references public.stellarion_games(id) on delete cascade,
    sequence bigint not null,
    kind text not null,
    revision bigint,
    turn bigint,
    player_id bigint,
    created_at timestamptz not null default clock_timestamp(),
    primary key (game_id, sequence),
    constraint stellarion_game_events_kind check (
        kind in (
            'player_joined',
            'player_recovered',
            'player_connected',
            'player_disconnected',
            'game_resumed',
            'turn_submitted',
            'turn_withdrawn',
            'protection_changed',
            'joint_attack_changed',
            'trade_changed',
            'state_changed',
            'game_started',
            'turn_resolved',
            'game_finished'
        )
    ),
    constraint stellarion_game_events_revision check (revision is null or revision >= 0),
    constraint stellarion_game_events_turn check (turn is null or turn >= 1),
    constraint stellarion_game_events_player check (player_id is null or player_id between 1 and 4)
);

create index stellarion_game_events_replay
    on public.stellarion_game_events (game_id, sequence);

-- Returns whether the JWT currently executing the query owns a slot in a
-- game. Keeping this lookup in a definer function avoids recursive RLS on the
-- membership table.
create function public.stellarion_is_game_member(p_game_id uuid)
returns boolean
language sql
stable
security definer
set search_path = pg_catalog, public, auth
as $$
    select auth.uid() is not null
       and exists (
            select 1
            from public.stellarion_game_players as gp
            where gp.game_id = p_game_id
              and gp.user_id = auth.uid()
       );
$$;

-- Realtime evaluates this row policy before delivering an event. Joint-attack wake-ups are
-- addressed to one participant at a time, so uninvolved members cannot infer the operation.
create function public.stellarion_can_read_event(
    p_game_id uuid,
    p_kind text,
    p_player_id bigint
)
returns boolean
language sql
stable
security definer
set search_path = pg_catalog, public, auth
as $$
    select exists (
        select 1 from public.stellarion_game_players gp
         where gp.game_id = p_game_id
           and gp.user_id = auth.uid()
           and (p_kind not in ('joint_attack_changed', 'trade_changed')
                or gp.player_id = p_player_id)
    );
$$;

alter table public.stellarion_games enable row level security;
alter table public.stellarion_game_players enable row level security;
alter table public.stellarion_turn_submissions enable row level security;
alter table public.stellarion_joint_attacks enable row level security;
alter table public.stellarion_trades enable row level security;
alter table public.stellarion_game_events enable row level security;

create policy stellarion_games_member_select
    on public.stellarion_games
    for select
    to authenticated
    using (public.stellarion_is_game_member(id));

create policy stellarion_players_member_select
    on public.stellarion_game_players
    for select
    to authenticated
    using (public.stellarion_is_game_member(game_id));

create policy stellarion_submissions_member_select
    on public.stellarion_turn_submissions
    for select
    to authenticated
    using (public.stellarion_is_game_member(game_id));

create policy stellarion_events_member_select
    on public.stellarion_game_events
    for select
    to authenticated
    using (public.stellarion_can_read_event(game_id, kind, player_id));

-- Validates the database-visible invariants of the Rust snapshot.
-- Detailed gameplay validation remains in the deterministic Rust core.
create function public.stellarion_validate_persisted(
    p_persisted jsonb,
    p_max_players smallint,
    p_expected_status text,
    p_expected_turn bigint default null
)
returns void
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    v_player_count integer;
    v_turn bigint;
    v_status text;
    v_players jsonb;
    v_total integer;
    v_unique integer;
    v_unique_colors integer;
    v_min_id bigint;
    v_max_id bigint;
    v_planets_per_player integer;
    v_colonizable_percent integer;
    v_moons_percent integer;
    v_planets jsonb;
    v_missions jsonb;
    v_orbital_strikes jsonb;
    v_trades jsonb;
    v_planet_total integer;
    v_unique_planets integer;
    v_min_planet_id bigint;
    v_max_planet_id bigint;
begin
    if p_persisted is null
       or jsonb_typeof(p_persisted) is distinct from 'object'
       or pg_column_size(p_persisted) > 67108864
       or jsonb_typeof(p_persisted -> 'state') is distinct from 'object'
       or not (p_persisted ?& array['state'])
       or p_persisted - array['state'] <> '{}'::jsonb
       or not ((p_persisted -> 'state') ?&
           array['players', 'map', 'missions', 'orbital_strikes', 'trades', 'turn', 'rng', 'rules', 'status'])
       or (p_persisted -> 'state') -
           array['players', 'map', 'missions', 'orbital_strikes', 'trades', 'turn', 'rng', 'rules', 'status'] <> '{}'::jsonb then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:persisted object';
    end if;

    v_player_count := (p_persisted #>> '{state,rules,player_count}')::integer;
    v_turn := (p_persisted #>> '{state,turn}')::bigint;
    v_status := p_persisted #>> '{state,status}';
    v_players := p_persisted #> '{state,players}';
    v_planets_per_player := (p_persisted #>> '{state,rules,planets_per_player}')::integer;
    v_colonizable_percent := (p_persisted #>> '{state,rules,colonizable_percent}')::integer;
    v_moons_percent := (p_persisted #>> '{state,rules,moons_percent}')::integer;
    v_planets := p_persisted #> '{state,map,planets}';
    v_missions := p_persisted #> '{state,missions}';
    v_orbital_strikes := p_persisted #> '{state,orbital_strikes}';
    v_trades := p_persisted #> '{state,trades}';

    if p_max_players is null
       or p_max_players not between 2 and 4
       or v_player_count is distinct from p_max_players then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:player_count';
    end if;
    if v_turn is null
       or v_turn < 1
       or (p_expected_turn is not null and v_turn <> p_expected_turn) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:turn';
    end if;
    if p_expected_status is null
       or p_expected_status not in ('lobby', 'active', 'finished')
       or v_status is distinct from p_expected_status
       or v_status not in ('lobby', 'active', 'finished') then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:status';
    end if;
    if jsonb_typeof(p_persisted #> '{state,rules}') is distinct from 'object'
       or not ((p_persisted #> '{state,rules}') ?&
           array['planets_per_player', 'colonizable_percent', 'moons_percent', 'player_count', 'practice_mode'])
       or (p_persisted #> '{state,rules}') -
           array['planets_per_player', 'colonizable_percent', 'moons_percent', 'player_count', 'practice_mode'] <> '{}'::jsonb
       or p_persisted #> '{state,rules,practice_mode}' is distinct from 'false'::jsonb
       or v_planets_per_player is null or v_planets_per_player not between 5 and 20
       or v_colonizable_percent is null or v_colonizable_percent not in (25, 35, 50)
       or v_moons_percent is null or v_moons_percent not between 0 and 100 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:rules';
    end if;
    if jsonb_typeof(v_players) is distinct from 'array' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:players';
    end if;
    if jsonb_typeof(v_planets) is distinct from 'array' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:planets';
    end if;
    if jsonb_typeof(v_missions) is distinct from 'array' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:missions';
    end if;
    if jsonb_array_length(v_missions) > 4096 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:missions';
    end if;
    if jsonb_typeof(v_orbital_strikes) is distinct from 'array' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:orbital_strikes';
    end if;
    if jsonb_array_length(v_orbital_strikes) > 160 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:orbital_strikes';
    end if;

    if exists (
        select 1
        from jsonb_array_elements(v_players) as entries(entry)
        where case
            when jsonb_typeof(entry) is distinct from 'object' then true
            when not (entry ?& array[
                'id', 'home_planet', 'world_acquisition_order', 'resources',
                'reports', 'protection_intel', 'spectator', 'color'
            ]) then true
            when entry - array[
                'id', 'home_planet', 'world_acquisition_order', 'resources',
                'reports', 'protection_intel', 'spectator', 'color'
            ] <> '{}'::jsonb then true
            when jsonb_typeof(entry -> 'spectator') is distinct from 'boolean' then true
            when jsonb_typeof(entry -> 'protection_intel') is distinct from 'object' then true
            when jsonb_typeof(entry -> 'color') is distinct from 'number' then true
            when entry ->> 'color' !~ '^[0-5]$' then true
            else false
        end
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:players';
    end if;

    if exists (
        select 1
        from jsonb_array_elements(v_players) as entries(entry)
        where case
            when jsonb_typeof(entry -> 'reports') is distinct from 'array' then true
            else jsonb_array_length(entry -> 'reports') > 512
        end
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:reports';
    end if;

    select count(*),
           count(distinct (entry ->> 'id')::bigint),
           count(distinct (entry ->> 'color')::integer),
           min((entry ->> 'id')::bigint),
           max((entry ->> 'id')::bigint)
      into v_total, v_unique, v_unique_colors, v_min_id, v_max_id
      from jsonb_array_elements(v_players) as entries(entry);

    if v_total <> p_max_players
       or v_unique <> p_max_players
       or v_min_id <> 1
       or v_max_id <> p_max_players then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:player_ids';
    end if;
    if v_unique_colors <> p_max_players then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:color_unavailable';
    end if;

    select count(*),
           count(distinct (entry ->> 'id')::bigint),
           min((entry ->> 'id')::bigint),
           max((entry ->> 'id')::bigint)
      into v_planet_total, v_unique_planets, v_min_planet_id, v_max_planet_id
      from jsonb_array_elements(v_planets) as entries(entry);

    if v_planet_total < p_max_players
       or v_planet_total > 160
       or v_unique_planets <> v_planet_total
       or v_min_planet_id <> 0
       or v_max_planet_id <> v_planet_total - 1 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:planet_ids';
    end if;
exception
    when invalid_text_representation or numeric_value_out_of_range then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:numeric_field';
end;
$$;

-- Presence is a renewable lease, not a saved fact. Closing a window or losing the
-- network can prevent an explicit disconnect. All roster responses, resume checks,
-- and recovery guards therefore use the same 15-second heartbeat deadline. Clients
-- renew every 5 seconds and receive only this small roster, never the game snapshot.
-- Read-only loads never extend the lease, and expiry does not delete a saved game.
create function public.stellarion_connection_is_live(
    p_connected boolean,
    p_last_seen_at timestamptz
)
returns boolean
language sql
volatile
set search_path = pg_catalog
as $$
    select p_connected and p_last_seen_at > clock_timestamp() - interval '15 seconds';
$$;

-- Compact roster projection used by both full records and frequent heartbeats.
-- Keeping it separate prevents presence refreshes from retransmitting the persisted
-- map, missions, and combat history every few seconds.
create function public.stellarion_membership_records(p_game_id uuid)
returns jsonb
language sql
volatile
set search_path = pg_catalog, public
as $$
    select coalesce(
        jsonb_agg(
            jsonb_build_object(
                'game_id', gp.game_id::text,
                'player_id', gp.player_id,
                'user_id', gp.user_id::text,
                'display_name', gp.display_name,
                'is_creator', gp.is_creator,
                'identity_version', gp.identity_version,
                'connected', public.stellarion_connection_is_live(
                    gp.connected, gp.last_seen_at
                )
            ) order by gp.player_id
        ),
        '[]'::jsonb
    )
    from public.stellarion_game_players as gp
    where gp.game_id = p_game_id;
$$;

-- Builds the exact JSON shape consumed by multiplayer::model::GameRecord.
create function public.stellarion_game_record(p_game_id uuid)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    v_result jsonb;
begin
    select jsonb_build_object(
               'id', g.id::text,
               'code', g.code,
               'revision', g.revision,
               'saved_at', floor(extract(epoch from g.saved_at))::bigint,
               'max_players', g.max_players,
               'status', g.status,
               'persisted', g.state,
               'submitted_players', coalesce((select jsonb_agg(s.player_id order by s.player_id) from public.stellarion_turn_submissions s where s.game_id = g.id and s.turn = g.current_turn and s.ready), '[]'::jsonb),
               'members', public.stellarion_membership_records(g.id)
           )
      into v_result
      from public.stellarion_games as g
      where g.id = p_game_id;

    if v_result is null then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    return v_result;
end;
$$;

-- Builds the exact JSON shape consumed by GameMembership.
create function public.stellarion_membership_record(
    p_game_id uuid,
    p_user_id uuid
)
returns jsonb
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    v_result jsonb;
begin
    select jsonb_build_object(
               'game_id', gp.game_id::text,
               'player_id', gp.player_id,
               'user_id', gp.user_id::text,
               'display_name', gp.display_name,
               'is_creator', gp.is_creator,
               'identity_version', gp.identity_version,
               'connected', public.stellarion_connection_is_live(gp.connected, gp.last_seen_at)
           )
      into v_result
      from public.stellarion_game_players as gp
      where gp.game_id = p_game_id
        and gp.user_id = p_user_id;

    if v_result is null then
        raise exception using errcode = 'P0001', message = 'STLR_PLAYER_REMOVED';
    end if;
    return v_result;
end;
$$;

-- Appends a monotonic durable event and retains a bounded reconnect history.
create function public.stellarion_emit_event(
    p_game_id uuid,
    p_kind text,
    p_turn bigint default null,
    p_player_id bigint default null
)
returns bigint
language plpgsql
set search_path = pg_catalog, public
as $$
declare
    v_sequence bigint;
    v_revision bigint;
begin
    update public.stellarion_games
       set event_sequence = event_sequence + 1,
           updated_at = clock_timestamp()
     where id = p_game_id
     returning event_sequence, revision into v_sequence, v_revision;

    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;

    insert into public.stellarion_game_events (
        game_id, sequence, kind, revision, turn, player_id
    ) values (
        p_game_id, v_sequence, p_kind, v_revision, p_turn, p_player_id
    );

    -- Realtime is only a wake-up path. Clients replay these durable semantic
    -- events and reload the full snapshot only when an event changed it. Keeping
    -- the latest 2,048 events prevents an abandoned game growing forever.
    delete from public.stellarion_game_events
     where game_id = p_game_id
       and sequence <= v_sequence - 2048;

    return v_sequence;
end;
$$;

create function public.stellarion_create_game(
    p_code text,
    p_display_name text,
    p_recovery_code text,
    p_max_players smallint,
    p_persisted jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, extensions, auth
as $$
declare
    v_user_id uuid := auth.uid();
    v_game_id uuid;
    v_code text := upper(btrim(p_code));
    v_name text := btrim(p_display_name);
    v_recovery_code text := upper(btrim(p_recovery_code));
begin
    if v_user_id is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if v_code is null or v_code !~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{6}$' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:game_code';
    end if;
    if v_name is null or char_length(v_name) not between 1 and 16 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:display_name';
    end if;
    if v_recovery_code is null
       or v_recovery_code !~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{4}(-[0-9ABCDEFGHJKMNPQRSTVWXYZ]{4}){3}$' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:recovery_code';
    end if;

    perform public.stellarion_validate_persisted(
        p_persisted, p_max_players, 'lobby', 1
    );

    begin
        insert into public.stellarion_games (
            code,
            created_by,
            max_players,
            status,
            state,
            current_turn
        ) values (
            v_code,
            v_user_id,
            p_max_players,
            'lobby',
            p_persisted,
            (p_persisted #>> '{state,turn}')::bigint
        )
        returning id into v_game_id;
    exception
        when unique_violation then
            raise exception using errcode = 'P0001', message = 'STLR_CODE_COLLISION';
    end;

    insert into public.stellarion_game_players (
        game_id, player_id, user_id, display_name, recovery_code, is_creator
    ) values (
        v_game_id, 1, v_user_id, v_name, v_recovery_code, true
    );

    perform public.stellarion_emit_event(v_game_id, 'player_joined', null, 1);

    return jsonb_build_object(
        'game', public.stellarion_game_record(v_game_id),
        'membership', public.stellarion_membership_record(v_game_id, v_user_id),
        'recovery_code', v_recovery_code,
        'disposition', 'joined'
    );
end;
$$;

create function public.stellarion_join_game(
    p_code text,
    p_display_name text,
    p_recovery_code text
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, extensions, auth
as $$
declare
    v_user_id uuid := auth.uid();
    v_game public.stellarion_games%rowtype;
    v_existing public.stellarion_game_players%rowtype;
    v_player_id bigint;
    v_code text := upper(btrim(p_code));
    v_name text := btrim(p_display_name);
    v_recovery_code text := upper(btrim(p_recovery_code));
begin
    if v_user_id is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if v_code is null or v_code !~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{6}$'
       or v_name is null or char_length(v_name) not between 1 and 16
       or v_recovery_code is null
       or v_recovery_code !~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{4}(-[0-9ABCDEFGHJKMNPQRSTVWXYZ]{4}){3}$' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:join_fields';
    end if;

    select * into v_game
      from public.stellarion_games
      where code = v_code
      for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;

    select * into v_existing
      from public.stellarion_game_players
      where game_id = v_game.id and user_id = v_user_id;
    if found then
        return jsonb_build_object(
            'game', public.stellarion_game_record(v_game.id),
            'membership', public.stellarion_membership_record(v_game.id, v_user_id),
            'recovery_code', v_existing.recovery_code,
            'disposition', 'reconnected'
        );
    end if;

    if v_game.status <> 'lobby' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_STATUS';
    end if;
    if (select count(*) from public.stellarion_game_players where game_id = v_game.id)
       >= v_game.max_players then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_FULL';
    end if;

    select slot into v_player_id
      from generate_series(1, v_game.max_players::integer) as available(slot)
      where not exists (
          select 1
          from public.stellarion_game_players as gp
          where gp.game_id = v_game.id and gp.player_id = available.slot
      )
      order by slot
      limit 1;
    if v_player_id is null then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_FULL';
    end if;

    begin
        insert into public.stellarion_game_players (
            game_id, player_id, user_id, display_name, recovery_code
        ) values (
            v_game.id, v_player_id, v_user_id, v_name, v_recovery_code
        );
    exception
        when unique_violation then
            -- The game row lock serializes slot claims. A remaining violation
            -- means the caller reused a recovery code or identity unexpectedly.
            raise exception using errcode = 'P0001', message = 'STLR_ALREADY_MEMBER';
    end;

    perform public.stellarion_emit_event(v_game.id, 'player_joined', null, v_player_id);
    return jsonb_build_object(
        'game', public.stellarion_game_record(v_game.id),
        'membership', public.stellarion_membership_record(v_game.id, v_user_id),
        'recovery_code', v_recovery_code,
        'disposition', 'joined'
    );
end;
$$;

-- Every player has one stable private code for the lifetime of this game. Recovery
-- rebinds the player slot without changing that code. A live player cannot be
-- displaced by recovery. Clients renew presence every 5 seconds;
-- after an unexpected close, recovery is available after 15 seconds without a
-- heartbeat. Leaving the game releases it immediately. The game/member locks also
-- serialize competing claims, and recovery claims presence before returning.
create function public.stellarion_recover_player(
    p_code text,
    p_recovery_code text
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, extensions, auth
as $$
declare
    v_user_id uuid := auth.uid();
    v_game_id uuid;
    v_player public.stellarion_game_players%rowtype;
    v_code text := upper(btrim(p_code));
    v_recovery_code text := upper(btrim(p_recovery_code));
begin
    if v_user_id is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if v_code is null or v_code !~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{6}$'
       or v_recovery_code is null
       or v_recovery_code !~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{4}(-[0-9ABCDEFGHJKMNPQRSTVWXYZ]{4}){3}$' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:recovery_code';
    end if;

    select id into v_game_id
      from public.stellarion_games
      where code = v_code
      for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if exists (
        select 1 from public.stellarion_game_players
        where game_id = v_game_id and user_id = v_user_id
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_ALREADY_MEMBER';
    end if;

    select * into v_player
      from public.stellarion_game_players
      where game_id = v_game_id and recovery_code = v_recovery_code
      for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_RECOVERY';
    end if;
    if public.stellarion_connection_is_live(v_player.connected, v_player.last_seen_at) then
        raise exception using errcode = 'P0001', message = 'STLR_RECOVERY_IN_USE';
    end if;

    begin
        update public.stellarion_game_players
           set user_id = v_user_id,
               identity_version = identity_version + 1,
               connected = true,
               last_seen_at = clock_timestamp()
         where game_id = v_game_id and player_id = v_player.player_id;
    exception
        when unique_violation then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_RECOVERY';
    end;

    perform public.stellarion_emit_event(v_game_id, 'player_recovered', null, v_player.player_id);
    return jsonb_build_object(
        'game', public.stellarion_game_record(v_game_id),
        'membership', public.stellarion_membership_record(v_game_id, v_user_id),
        'recovery_code', v_recovery_code,
        'disposition', 'reconnected'
    );
end;
$$;

-- Lobbies exist only to coordinate a live host and guests. They are never saved
-- games: only active/finished matches appear in Resume Game. A host leaving an
-- unstarted lobby deletes its entire record through stellarion_set_connected.
-- Include the caller's game-specific name and selected empire color.
create function public.stellarion_list_games()
returns jsonb
language sql
stable
security definer
set search_path = pg_catalog, public, auth
as $$
    select case
        when auth.uid() is null then
            jsonb_build_array()
        else
            coalesce(
                jsonb_agg(
                    jsonb_build_object(
                        'id', g.id::text,
                        'code', g.code,
                        'revision', g.revision,
                        'saved_at', floor(extract(epoch from g.saved_at))::bigint,
                        'status', g.status,
                        'turn', g.current_turn,
                        'player_id', mine.player_id,
                        'display_name', mine.display_name,
                        'recovery_code', mine.recovery_code,
                        'player_color', (
                            select (player ->> 'color')::integer
                            from jsonb_array_elements(g.state -> 'state' -> 'players') as player
                            where (player ->> 'id')::bigint = mine.player_id
                        ),
                        'player_count', (
                            select count(*)
                            from public.stellarion_game_players as all_players
                            where all_players.game_id = g.id
                        ),
                        'max_players', g.max_players
                    ) order by g.saved_at desc, g.id
                ),
                '[]'::jsonb
            )
    end
    from public.stellarion_games as g
    join public.stellarion_game_players as mine
      on mine.game_id = g.id and mine.user_id = auth.uid()
    where g.status <> 'lobby'
      and g.saved_at > statement_timestamp() - interval '30 days'
      and (g.status <> 'finished'
           or g.finished_at is null
           or g.finished_at > statement_timestamp() - interval '48 hours');
$$;

create function public.stellarion_load_game(p_game_id uuid)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if not exists (select 1 from public.stellarion_games where id = p_game_id) then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if not exists (
        select 1 from public.stellarion_game_players
        where game_id = p_game_id and user_id = auth.uid()
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    return public.stellarion_game_record(p_game_id);
end;
$$;

-- Creates one immutable inviter draft plus a private list of invited player slots. Responses are
-- updated separately so every participant sees the same live panel without exposing other state.
create function public.stellarion_create_joint_attack(
    p_game_id uuid,
    p_invitation jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_player bigint;
    v_attack_id bigint;
    v_participants jsonb;
    v_participant jsonb;
    v_destination jsonb;
    v_total bigint;
    v_unique bigint;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if jsonb_typeof(v_trades) is distinct from 'array'
       or jsonb_array_length(v_trades) > 6 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trades';
    end if;
    select * into v_game from public.stellarion_games where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    select player_id into v_player from public.stellarion_game_players
     where game_id = p_game_id and user_id = auth.uid();
    if v_player is null then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if p_invitation is null
       or jsonb_typeof(p_invitation) is distinct from 'object'
       or not (p_invitation ?& array[
           'id', 'turn', 'inviter', 'destination', 'objective', 'bombing',
           'combat_probes', 'canceled', 'participants'
       ])
       or p_invitation - array[
           'id', 'turn', 'inviter', 'destination', 'objective', 'bombing',
           'combat_probes', 'canceled', 'participants'
       ] <> '{}'::jsonb
    then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:joint_attack';
    end if;
    begin
        v_attack_id := (p_invitation ->> 'id')::bigint;
    exception
        when invalid_text_representation or numeric_value_out_of_range then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:joint_attack';
    end;
    v_participants := p_invitation -> 'participants';
    if jsonb_typeof(v_participants) = 'array' then
        select count(*), count(distinct (participant ->> 'player_id')::bigint)
          into v_total, v_unique
          from jsonb_array_elements(v_participants) participant;
    end if;
    select planet into v_destination
      from jsonb_array_elements(v_game.state -> 'state' -> 'map' -> 'planets') planet
     where (planet ->> 'id')::bigint = (p_invitation ->> 'destination')::bigint;
    if v_game.status <> 'active'
       or (p_invitation ->> 'turn')::bigint <> v_game.current_turn
       or (p_invitation ->> 'inviter')::bigint <> v_player
       or coalesce((p_invitation ->> 'canceled')::boolean, true)
       or (p_invitation ->> 'objective') not in ('Colonize', 'Attack', 'Destroy')
       or v_attack_id <= 0
       or jsonb_typeof(v_participants) is distinct from 'array'
       or jsonb_array_length(v_participants) < 2
       or jsonb_array_length(v_participants) > 4
       or v_total <> v_unique
       or v_destination is null
       or coalesce((v_destination ->> 'is_destroyed')::boolean, false)
       or (v_destination ->> 'owned')::bigint = v_player
       or (v_destination ->> 'controlled')::bigint = v_player
       or (v_participants -> 0 ->> 'player_id')::bigint <> v_player
       or (v_participants -> 0 ->> 'response') <> 'accepted'
       or jsonb_typeof(v_participants -> 0 -> 'contribution' -> 'army') is distinct from 'object'
       or (v_participants -> 0 -> 'contribution' ->> 'player_id')::bigint <> v_player
       or v_participants -> 0 -> 'contribution' -> 'army' = '{}'::jsonb
       or ((p_invitation ->> 'objective') = 'Colonize'
           and coalesce((v_participants -> 0 -> 'contribution' -> 'army'
                         ->> 'Ship(ColonyShip)')::bigint, 0) < 1)
       or ((p_invitation ->> 'objective') = 'Destroy'
           and coalesce((v_participants -> 0 -> 'contribution' -> 'army'
                         ->> 'Ship(WarSun)')::bigint, 0) < 1)
       or exists (
           select 1
             from jsonb_array_elements(v_participants) with ordinality
                  as entries(participant, ordinal)
            where jsonb_typeof(participant) is distinct from 'object'
               or not (participant ?& array['player_id', 'response', 'contribution'])
               or participant - array['player_id', 'response', 'contribution'] <> '{}'::jsonb
               or (ordinal > 1 and (
                   participant ->> 'response' <> 'pending'
                   or participant -> 'contribution' <> 'null'::jsonb
               ))
       )
       or exists (
           select 1
             from jsonb_array_elements(v_participants) as participant
            where not exists (
                select 1 from public.stellarion_game_players gp
                 where gp.game_id = p_game_id
                   and gp.player_id = (participant ->> 'player_id')::bigint
            )
       )
    then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:joint_attack';
    end if;
    insert into public.stellarion_joint_attacks(
        game_id, attack_id, turn, inviter, invitation
    ) values (
        p_game_id, v_attack_id, v_game.current_turn, v_player, p_invitation
    );
    for v_participant in select value from jsonb_array_elements(v_participants)
    loop
        perform public.stellarion_emit_event(
            p_game_id, 'joint_attack_changed', v_game.current_turn,
            (v_participant ->> 'player_id')::bigint
        );
    end loop;
    return p_invitation;
exception
    when unique_violation then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:joint_attack_id';
end;
$$;

create function public.stellarion_respond_joint_attack(
    p_game_id uuid,
    p_attack_id bigint,
    p_response text,
    p_contribution jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_player bigint;
    v_row public.stellarion_joint_attacks%rowtype;
    v_planet jsonb;
    v_participants jsonb;
    v_participant jsonb;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    select player_id into v_player from public.stellarion_game_players
     where game_id = p_game_id and user_id = auth.uid();
    if v_player is null then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    select * into v_row from public.stellarion_joint_attacks
     where game_id = p_game_id and attack_id = p_attack_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if v_player = v_row.inviter
       or coalesce((v_row.invitation ->> 'canceled')::boolean, false)
       or p_response not in ('accepted', 'rejected')
       or not exists (
           select 1 from jsonb_array_elements(v_row.invitation -> 'participants') participant
            where (participant ->> 'player_id')::bigint = v_player
              and (participant ->> 'response') = 'pending'
       )
    then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if p_response = 'accepted' then
        select planet into v_planet
          from public.stellarion_games game_row,
               jsonb_array_elements(game_row.state -> 'state' -> 'map' -> 'planets') planet
         where game_row.id = p_game_id
           and (planet ->> 'id')::bigint = (v_row.invitation ->> 'destination')::bigint;
        if v_planet is null
           or (v_planet ->> 'owned')::bigint = v_player
           or (v_planet ->> 'controlled')::bigint = v_player
           or p_contribution is null
           or (p_contribution ->> 'player_id')::bigint <> v_player
           or jsonb_typeof(p_contribution -> 'army') is distinct from 'object'
        then
            raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
        end if;
    end if;
    select jsonb_agg(
               case when (participant ->> 'player_id')::bigint = v_player
                    then jsonb_build_object(
                        'player_id', v_player,
                        'response', p_response,
                        'contribution', case when p_response = 'accepted'
                                             then p_contribution else 'null'::jsonb end
                    )
                    else participant end
               order by ordinal
           )
      into v_participants
      from jsonb_array_elements(v_row.invitation -> 'participants')
           with ordinality as entries(participant, ordinal);
    v_row.invitation := jsonb_set(v_row.invitation, '{participants}', v_participants, false);
    update public.stellarion_joint_attacks
       set invitation = v_row.invitation, updated_at = clock_timestamp()
     where game_id = p_game_id and attack_id = p_attack_id;
    for v_participant in select value from jsonb_array_elements(v_participants)
    loop
        perform public.stellarion_emit_event(
            p_game_id, 'joint_attack_changed', v_row.turn,
            (v_participant ->> 'player_id')::bigint
        );
    end loop;
    return v_row.invitation;
end;
$$;

-- The inviter may withdraw an invitation while it is still only a coordination draft. The row is
-- retained through the current turn so every invited player can reload the durable cancellation.
create function public.stellarion_cancel_joint_attack(
    p_game_id uuid,
    p_attack_id bigint
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_player bigint;
    v_row public.stellarion_joint_attacks%rowtype;
    v_participant jsonb;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    select player_id into v_player from public.stellarion_game_players
     where game_id = p_game_id and user_id = auth.uid();
    if v_player is null then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    select * into v_row from public.stellarion_joint_attacks
     where game_id = p_game_id and attack_id = p_attack_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if v_player <> v_row.inviter
       or coalesce((v_row.invitation ->> 'canceled')::boolean, false)
       or v_row.turn <> (
           select current_turn from public.stellarion_games where id = p_game_id
       )
       or exists (
           select 1
             from public.stellarion_turn_submissions submission
            where submission.game_id = p_game_id
              and submission.turn = v_row.turn
              and submission.player_id = v_player
              and exists (
                  select 1
                    from jsonb_array_elements(submission.submission -> 'commands') command
                   where command ->> 'kind' = 'send_joint_mission'
                     and (command ->> 'attack_id')::bigint = p_attack_id
              )
       )
    then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    v_row.invitation := jsonb_set(v_row.invitation, '{canceled}', 'true'::jsonb, false);
    update public.stellarion_joint_attacks
       set invitation = v_row.invitation, updated_at = clock_timestamp()
     where game_id = p_game_id and attack_id = p_attack_id;
    for v_participant in
        select value from jsonb_array_elements(v_row.invitation -> 'participants')
    loop
        perform public.stellarion_emit_event(
            p_game_id, 'joint_attack_changed', v_row.turn,
            (v_participant ->> 'player_id')::bigint
        );
    end loop;
    return v_row.invitation;
end;
$$;

create function public.stellarion_load_joint_attacks(p_game_id uuid)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_player bigint;
    v_turn bigint;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    select gp.player_id, g.current_turn into v_player, v_turn
      from public.stellarion_game_players gp
      join public.stellarion_games g on g.id = gp.game_id
     where gp.game_id = p_game_id and gp.user_id = auth.uid();
    if v_player is null then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    return coalesce((
        select jsonb_agg(ja.invitation order by ja.attack_id)
          from public.stellarion_joint_attacks ja
         where ja.game_id = p_game_id
           and ja.turn = v_turn
           and exists (
               select 1 from jsonb_array_elements(ja.invitation -> 'participants') participant
                where (participant ->> 'player_id')::bigint = v_player
           )
    ), '[]'::jsonb);
end;
$$;

-- These compact helpers validate untrusted trade JSON without letting malformed numeric fields
-- escape as implementation-specific PostgreSQL errors.
create function public.stellarion_trade_resources_valid(p_resources jsonb, p_allow_empty boolean)
returns boolean
language plpgsql
immutable
set search_path = pg_catalog
as $$
declare
    v_total bigint;
begin
    if jsonb_typeof(p_resources) is distinct from 'object'
       or not (p_resources ?& array['metal', 'crystal', 'deuterium'])
       or p_resources - array['metal', 'crystal', 'deuterium'] <> '{}'::jsonb
       or p_resources ->> 'metal' !~ '^[0-9]+$'
       or p_resources ->> 'crystal' !~ '^[0-9]+$'
       or p_resources ->> 'deuterium' !~ '^[0-9]+$' then
        return false;
    end if;
    v_total := (p_resources ->> 'metal')::bigint
        + (p_resources ->> 'crystal')::bigint
        + (p_resources ->> 'deuterium')::bigint;
    return v_total >= case when p_allow_empty then 0 else 1 end;
exception
    when invalid_text_representation or numeric_value_out_of_range then
        return false;
end;
$$;

create function public.stellarion_trade_capacity(p_planet jsonb, p_player bigint)
returns bigint
language plpgsql
immutable
set search_path = pg_catalog
as $$
declare
    v_level bigint;
begin
    if jsonb_typeof(p_planet) is distinct from 'object'
       or (p_planet ->> 'owned')::bigint is distinct from p_player
       or coalesce((p_planet ->> 'is_destroyed')::boolean, true)
       or p_planet ->> 'kind' in ('Blue', 'Brown', 'Gray', 'Red', 'Yellow') then
        return 0;
    end if;
    v_level := coalesce((p_planet #>> '{army,controller,Building(TradingPost)}')::bigint, 0);
    return least(v_level, 3) * 500;
exception
    when invalid_text_representation or numeric_value_out_of_range then
        return 0;
end;
$$;

create function public.stellarion_trade_route_valid(p_first jsonb, p_second jsonb)
returns boolean
language plpgsql
immutable
set search_path = pg_catalog
as $$
declare
    v_dx double precision;
    v_dy double precision;
begin
    if (p_first ->> 'id')::bigint = (p_second ->> 'id')::bigint
       or jsonb_typeof(p_first -> 'position') is distinct from 'array'
       or jsonb_typeof(p_second -> 'position') is distinct from 'array'
       or jsonb_array_length(p_first -> 'position') <> 2
       or jsonb_array_length(p_second -> 'position') <> 2 then
        return false;
    end if;
    v_dx := (p_first #>> '{position,0}')::double precision
        - (p_second #>> '{position,0}')::double precision;
    v_dy := (p_first #>> '{position,1}')::double precision
        - (p_second #>> '{position,1}')::double precision;
    -- Planet::SIZE is 100 world units, so three AU is a 300-unit center distance.
    return v_dx * v_dx + v_dy * v_dy <= 90000.0;
exception
    when invalid_text_representation or numeric_value_out_of_range then
        return false;
end;
$$;

create function public.stellarion_create_trade(p_game_id uuid, p_invitation jsonb)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_player bigint;
    v_trade_id bigint;
    v_participants jsonb;
    v_first jsonb;
    v_second jsonb;
    v_first_planet jsonb;
    v_second_planet jsonb;
    v_participant jsonb;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    select * into v_game from public.stellarion_games where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    select player_id into v_player from public.stellarion_game_players
     where game_id = p_game_id and user_id = auth.uid();
    if v_player is null then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if p_invitation is null
       or jsonb_typeof(p_invitation) is distinct from 'object'
       or not (p_invitation ?& array[
           'id', 'turn', 'proposer', 'canceled', 'finalized', 'participants'
       ])
       or p_invitation - array[
           'id', 'turn', 'proposer', 'canceled', 'finalized', 'participants'
       ] <> '{}'::jsonb then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trade';
    end if;
    v_trade_id := (p_invitation ->> 'id')::bigint;
    v_participants := p_invitation -> 'participants';
    if jsonb_typeof(v_participants) is distinct from 'array'
       or jsonb_array_length(v_participants) <> 2 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trade';
    end if;
    v_first := v_participants -> 0;
    v_second := v_participants -> 1;
    select planet into v_first_planet
      from jsonb_array_elements(v_game.state #> '{state,map,planets}') planet
     where (planet ->> 'id')::bigint = (v_first ->> 'planet_id')::bigint;
    select planet into v_second_planet
      from jsonb_array_elements(v_game.state #> '{state,map,planets}') planet
     where (planet ->> 'id')::bigint = (v_second ->> 'planet_id')::bigint;
    if v_game.status <> 'active'
       or v_trade_id <= 0
       or (p_invitation ->> 'turn')::bigint <> v_game.current_turn
       or (p_invitation ->> 'proposer')::bigint <> v_player
       or coalesce((p_invitation ->> 'canceled')::boolean, true)
       or coalesce((p_invitation ->> 'finalized')::boolean, true)
       or (v_first ->> 'player_id')::bigint >= (v_second ->> 'player_id')::bigint
       or not exists (
           select 1 from jsonb_array_elements(v_participants) participant
            where (participant ->> 'player_id')::bigint = v_player
              and participant ->> 'response' = 'accepted'
              and public.stellarion_trade_resources_valid(participant -> 'resources', false)
       )
       or exists (
           select 1 from jsonb_array_elements(v_participants) participant
            where jsonb_typeof(participant) is distinct from 'object'
               or not (participant ?& array['player_id', 'planet_id', 'resources', 'response'])
               or participant - array['player_id', 'planet_id', 'resources', 'response'] <> '{}'::jsonb
               or not exists (
                   select 1 from public.stellarion_game_players gp
                    where gp.game_id = p_game_id
                      and gp.player_id = (participant ->> 'player_id')::bigint
               )
               or ((participant ->> 'player_id')::bigint <> v_player and (
                   participant ->> 'response' <> 'pending'
                   or not public.stellarion_trade_resources_valid(participant -> 'resources', true)
                   or participant -> 'resources' <> jsonb_build_object(
                       'metal', 0, 'crystal', 0, 'deuterium', 0
                   )
               ))
       )
       or v_first_planet is null or v_second_planet is null
       or public.stellarion_trade_capacity(
           v_first_planet, (v_first ->> 'player_id')::bigint
       ) <= 0
       or public.stellarion_trade_capacity(
           v_second_planet, (v_second ->> 'player_id')::bigint
       ) <= 0
       or not public.stellarion_trade_route_valid(v_first_planet, v_second_planet)
       or exists (
           select 1 from jsonb_array_elements(v_participants) participant
            where participant ->> 'response' = 'accepted'
              and ((participant -> 'resources' ->> 'metal')::bigint
                  + (participant -> 'resources' ->> 'crystal')::bigint
                  + (participant -> 'resources' ->> 'deuterium')::bigint)
                  > case when (participant ->> 'player_id')::bigint =
                                  (v_first ->> 'player_id')::bigint
                         then public.stellarion_trade_capacity(
                             v_first_planet, (v_first ->> 'player_id')::bigint
                         )
                         else public.stellarion_trade_capacity(
                             v_second_planet, (v_second ->> 'player_id')::bigint
                         ) end
       )
       or exists (
           select 1 from public.stellarion_turn_submissions submission
            where submission.game_id = p_game_id
              and submission.turn = v_game.current_turn
              and submission.ready
              and submission.player_id in (
                  (v_first ->> 'player_id')::bigint, (v_second ->> 'player_id')::bigint
              )
       ) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trade';
    end if;
    insert into public.stellarion_trades(
        game_id, trade_id, turn, player_low, player_high, invitation
    ) values (
        p_game_id, v_trade_id, v_game.current_turn,
        (v_first ->> 'player_id')::bigint, (v_second ->> 'player_id')::bigint, p_invitation
    );
    for v_participant in select value from jsonb_array_elements(v_participants)
    loop
        perform public.stellarion_emit_event(
            p_game_id, 'trade_changed', v_game.current_turn,
            (v_participant ->> 'player_id')::bigint
        );
    end loop;
    return p_invitation;
exception
    when unique_violation then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trade_pair';
    when invalid_text_representation or numeric_value_out_of_range then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trade';
end;
$$;

create function public.stellarion_respond_trade(
    p_game_id uuid,
    p_trade_id bigint,
    p_resources jsonb,
    p_response text
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_row public.stellarion_trades%rowtype;
    v_player bigint;
    v_participants jsonb;
    v_old_resources jsonb;
    v_first jsonb;
    v_second jsonb;
    v_first_planet jsonb;
    v_second_planet jsonb;
    v_participant jsonb;
    v_agreement jsonb;
    v_parties jsonb;
    v_state jsonb;
    v_stock jsonb;
    v_outgoing jsonb;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    select * into v_game from public.stellarion_games where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    select player_id into v_player from public.stellarion_game_players
     where game_id = p_game_id and user_id = auth.uid();
    if v_player is null then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    select * into v_row from public.stellarion_trades
     where game_id = p_game_id and trade_id = p_trade_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    select participant -> 'resources' into v_old_resources
      from jsonb_array_elements(v_row.invitation -> 'participants') participant
     where (participant ->> 'player_id')::bigint = v_player;
    if v_game.status <> 'active'
       or v_row.turn <> v_game.current_turn
       or coalesce((v_row.invitation ->> 'canceled')::boolean, false)
       or coalesce((v_row.invitation ->> 'finalized')::boolean, false)
       or p_response not in ('accepted', 'rejected')
       or v_old_resources is null
       or exists (
           select 1 from public.stellarion_turn_submissions submission
            where submission.game_id = p_game_id
              and submission.turn = v_game.current_turn
              and submission.ready
              and submission.player_id in (v_row.player_low, v_row.player_high)
       ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if p_response = 'accepted' and not public.stellarion_trade_resources_valid(p_resources, false) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trade_resources';
    end if;

    select jsonb_agg(
               case
                   when (participant ->> 'player_id')::bigint = v_player then
                       participant || jsonb_build_object(
                           'resources', case when p_response = 'accepted'
                                             then p_resources else participant -> 'resources' end,
                           'response', p_response
                       )
                   when p_response = 'accepted' and v_old_resources is distinct from p_resources then
                       participant || jsonb_build_object('response', 'pending')
                   else participant
               end order by ordinal
           ) into v_participants
      from jsonb_array_elements(v_row.invitation -> 'participants')
           with ordinality as entries(participant, ordinal);
    v_row.invitation := jsonb_set(v_row.invitation, '{participants}', v_participants, false);
    if p_response = 'rejected' then
        v_row.invitation := jsonb_set(v_row.invitation, '{canceled}', 'true'::jsonb, false);
    elsif not exists (
        select 1 from jsonb_array_elements(v_participants) participant
         where participant ->> 'response' <> 'accepted'
    ) then
        v_first := v_participants -> 0;
        v_second := v_participants -> 1;
        select planet into v_first_planet
          from jsonb_array_elements(v_game.state #> '{state,map,planets}') planet
         where (planet ->> 'id')::bigint = (v_first ->> 'planet_id')::bigint;
        select planet into v_second_planet
          from jsonb_array_elements(v_game.state #> '{state,map,planets}') planet
         where (planet ->> 'id')::bigint = (v_second ->> 'planet_id')::bigint;
        if v_first_planet is null or v_second_planet is null
           or not public.stellarion_trade_route_valid(v_first_planet, v_second_planet)
           or exists (
               select 1 from jsonb_array_elements(v_participants) participant
                where not public.stellarion_trade_resources_valid(participant -> 'resources', false)
                   or ((participant -> 'resources' ->> 'metal')::bigint
                       + (participant -> 'resources' ->> 'crystal')::bigint
                       + (participant -> 'resources' ->> 'deuterium')::bigint)
                       > case when (participant ->> 'player_id')::bigint = v_row.player_low
                              then public.stellarion_trade_capacity(v_first_planet, v_row.player_low)
                              else public.stellarion_trade_capacity(v_second_planet, v_row.player_high)
                         end
           ) then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trade_route';
        end if;

        -- Every outgoing component must remain available after earlier finalized trades.
        for v_participant in select value from jsonb_array_elements(v_participants)
        loop
            select player -> 'resources' into v_stock
              from jsonb_array_elements(v_game.state #> '{state,players}') player
             where (player ->> 'id')::bigint = (v_participant ->> 'player_id')::bigint;
            select jsonb_build_object(
                       'metal', coalesce(sum((party -> 'resources' ->> 'metal')::bigint), 0),
                       'crystal', coalesce(sum((party -> 'resources' ->> 'crystal')::bigint), 0),
                       'deuterium', coalesce(sum((party -> 'resources' ->> 'deuterium')::bigint), 0)
                   ) into v_outgoing
              from jsonb_array_elements(v_game.state #> '{state,trades}') trade,
                   jsonb_array_elements(trade -> 'parties') party
             where (party ->> 'player_id')::bigint = (v_participant ->> 'player_id')::bigint;
            if (v_outgoing ->> 'metal')::bigint
                   + (v_participant -> 'resources' ->> 'metal')::bigint
                   > (v_stock ->> 'metal')::bigint
               or (v_outgoing ->> 'crystal')::bigint
                   + (v_participant -> 'resources' ->> 'crystal')::bigint
                   > (v_stock ->> 'crystal')::bigint
               or (v_outgoing ->> 'deuterium')::bigint
                   + (v_participant -> 'resources' ->> 'deuterium')::bigint
                   > (v_stock ->> 'deuterium')::bigint then
                raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trade_balance';
            end if;
        end loop;

        select jsonb_agg(
                   jsonb_build_object(
                       'player_id', participant -> 'player_id',
                       'planet_id', participant -> 'planet_id',
                       'resources', participant -> 'resources'
                   ) order by ordinal
               ) into v_parties
          from jsonb_array_elements(v_participants)
               with ordinality as entries(participant, ordinal);
        v_agreement := jsonb_build_object(
            'id', p_trade_id, 'turn', v_game.current_turn, 'parties', v_parties
        );
        v_state := jsonb_set(
            v_game.state,
            '{state,trades}',
            (v_game.state #> '{state,trades}') || jsonb_build_array(v_agreement),
            false
        );
        perform public.stellarion_validate_persisted(
            v_state, v_game.max_players, v_game.status, v_game.current_turn
        );
        update public.stellarion_games
           set state = v_state, revision = revision + 1, updated_at = clock_timestamp()
         where id = p_game_id
         returning revision into v_game.revision;
        v_row.invitation := jsonb_set(v_row.invitation, '{finalized}', 'true'::jsonb, false);
    end if;

    update public.stellarion_trades
       set invitation = v_row.invitation, updated_at = clock_timestamp()
     where game_id = p_game_id and trade_id = p_trade_id;
    for v_participant in select value from jsonb_array_elements(v_participants)
    loop
        perform public.stellarion_emit_event(
            p_game_id, 'trade_changed', v_game.current_turn,
            (v_participant ->> 'player_id')::bigint
        );
    end loop;
    if coalesce((v_row.invitation ->> 'finalized')::boolean, false) then
        perform public.stellarion_emit_event(
            p_game_id, 'state_changed', v_game.current_turn, null
        );
    end if;
    return v_row.invitation;
exception
    when invalid_text_representation or numeric_value_out_of_range then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:trade';
end;
$$;

create function public.stellarion_load_trades(p_game_id uuid)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_player bigint;
    v_turn bigint;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    select gp.player_id, g.current_turn into v_player, v_turn
      from public.stellarion_game_players gp
      join public.stellarion_games g on g.id = gp.game_id
     where gp.game_id = p_game_id and gp.user_id = auth.uid();
    if v_player is null then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    return coalesce((
        select jsonb_agg(trade.invitation order by trade.trade_id)
          from public.stellarion_trades trade
         where trade.game_id = p_game_id
           and trade.turn = v_turn
           and v_player in (trade.player_low, trade.player_high)
    ), '[]'::jsonb);
end;
$$;

-- Protection invitations are coordination state, not simultaneous turn orders. This compact RPC
-- changes only one world/player relationship, records lasting controller intelligence for the
-- invited player, and immediately redirects travelling or stationed Protect fleets on revoke.
-- The game row lock serializes the patch against resolution without uploading the full snapshot.
create function public.stellarion_set_protection_permission(
    p_game_id uuid,
    p_planet_id bigint,
    p_protector bigint,
    p_allowed boolean
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_controller bigint;
    v_planet_index integer;
    v_player_index integer;
    v_planet jsonb;
    v_home jsonb;
    v_home_planet bigint;
    v_permissions jsonb;
    v_current boolean;
    v_state jsonb;
    v_missions jsonb;
    v_stationed jsonb;
    v_protectors jsonb;
    v_controller_army jsonb;
    v_mission_id bigint;
    v_position jsonb;
    v_dx double precision;
    v_dy double precision;
    v_length double precision;
    v_withdrew boolean := false;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if p_planet_id is null or p_planet_id not between 0 and 159
       or p_protector is null or p_protector not between 1 and 4
       or p_allowed is null then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:protection';
    end if;
    select * into v_game
      from public.stellarion_games
      where id = p_game_id
      for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if v_game.status <> 'active' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_STATUS';
    end if;
    select player_id into v_controller
      from public.stellarion_game_players
      where game_id = p_game_id and user_id = auth.uid();
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if p_protector = v_controller
       or (select count(*) from jsonb_array_elements(v_game.state #> '{state,players}') player
           where (player ->> 'spectator')::boolean = false) < 3
       or not exists (
           select 1 from jsonb_array_elements(v_game.state #> '{state,players}') player
           where (player ->> 'id')::bigint = p_protector
             and (player ->> 'spectator')::boolean = false
       ) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:protection_player';
    end if;

    select (ordinality - 1)::integer, planet
      into v_planet_index, v_planet
      from jsonb_array_elements(v_game.state #> '{state,map,planets}')
           with ordinality as planets(planet, ordinality)
      where (planet ->> 'id')::bigint = p_planet_id;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:protection_planet';
    end if;
    if (v_planet ->> 'is_destroyed')::boolean
       or (v_planet ->> 'controlled')::bigint is distinct from v_controller then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    v_permissions := v_planet -> 'protection_permissions';
    if jsonb_typeof(v_permissions) is distinct from 'array' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:protection_state';
    end if;
    v_current := exists (
        select 1 from jsonb_array_elements_text(v_permissions) permission
        where permission::bigint = p_protector
    );
    if v_current = p_allowed then
        return jsonb_build_object(
            'revision', v_game.revision,
            'turn', v_game.current_turn,
            'planet_id', p_planet_id,
            'controller', v_controller,
            'protector', p_protector,
            'allowed', p_allowed
        );
    end if;

    v_state := v_game.state;
    if p_allowed then
        v_permissions := v_permissions || jsonb_build_array(p_protector);
    else
        select coalesce(jsonb_agg(permission order by permission::text), '[]'::jsonb)
          into v_permissions
          from jsonb_array_elements(v_permissions) permission
          where (permission #>> '{}')::bigint <> p_protector;
    end if;
    v_planet := jsonb_set(v_planet, '{protection_permissions}', v_permissions, false);

    select (ordinality - 1)::integer,
           (player ->> 'home_planet')::bigint
      into v_player_index, v_home_planet
      from jsonb_array_elements(v_state #> '{state,players}')
           with ordinality as players(player, ordinality)
      where (player ->> 'id')::bigint = p_protector;
    select planet into v_home
      from jsonb_array_elements(v_state #> '{state,map,planets}') planet
      where (planet ->> 'id')::bigint = v_home_planet;
    v_state := jsonb_set(
        v_state,
        array['state', 'players', v_player_index::text, 'protection_intel', p_planet_id::text],
        to_jsonb(v_controller),
        true
    );

    if not p_allowed then
        v_missions := v_state #> '{state,missions}';
        v_stationed := v_planet #> array['army', 'protectors', p_protector::text];
        select coalesce(
                   jsonb_agg(
                       case
                           when (mission ->> 'owner')::bigint = p_protector
                            and (mission ->> 'destination')::bigint = p_planet_id
                            and mission ->> 'objective' = 'Protect'
                           then mission || jsonb_build_object(
                               'origin', p_planet_id,
                               'origin_owned', v_planet -> 'owned',
                               'origin_controlled', v_planet -> 'controlled',
                               'origin_army', coalesce(v_stationed, '{}'::jsonb),
                               'destination', v_home_planet,
                               'send', v_game.current_turn,
                               'travel_turns', 0,
                               'objective', 'Deploy',
                               'protected_player', null,
                               'return_objective', 'Protect',
                               'bombing', 'None',
                               'combat_probes', false,
                               'jump_gate', false,
                               'logs', (mission ->> 'logs') || E'\n- (' || v_game.current_turn::text
                                   || ') Protection access canceled; returning to home planet '
                                   || (v_home ->> 'name') || '.'
                           )
                           else mission
                       end order by ordinality
                   ),
                   '[]'::jsonb
               )
          into v_missions
          from jsonb_array_elements(v_missions) with ordinality as missions(mission, ordinality);

        if v_stationed is not null
           and jsonb_typeof(v_stationed) = 'object'
           and v_stationed <> '{}'::jsonb then
            v_protectors := (v_planet #> '{army,protectors}') - p_protector::text;
            v_planet := jsonb_set(v_planet, '{army,protectors}', v_protectors, false);
            if v_home_planet = p_planet_id then
                select coalesce(jsonb_object_agg(unit, amount), '{}'::jsonb)
                  into v_controller_army
                  from (
                      select unit, to_jsonb(sum((amount #>> '{}')::bigint)) as amount
                      from (
                          select * from jsonb_each(v_planet #> '{army,controller}')
                          union all
                          select * from jsonb_each(v_stationed)
                      ) armies(unit, amount)
                      group by unit
                  ) merged;
                v_planet := jsonb_set(v_planet, '{army,controller}', v_controller_army, false);
            elsif not (v_home ->> 'is_destroyed')::boolean then
                if jsonb_array_length(v_missions) >= 4096 then
                    raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:mission_limit';
                end if;
                select candidate into v_mission_id
                  from generate_series(1, jsonb_array_length(v_missions) + 1) candidate
                  where not exists (
                      select 1 from jsonb_array_elements(v_missions) mission
                      where (mission ->> 'id')::numeric = candidate
                  )
                  order by candidate
                  limit 1;
                v_dx := (v_home #>> '{position,0}')::double precision
                    - (v_planet #>> '{position,0}')::double precision;
                v_dy := (v_home #>> '{position,1}')::double precision
                    - (v_planet #>> '{position,1}')::double precision;
                v_length := sqrt(v_dx * v_dx + v_dy * v_dy);
                if v_length > 0 then
                    v_position := jsonb_build_array(
                        (v_planet #>> '{position,0}')::double precision + v_dx / v_length * 70.0,
                        (v_planet #>> '{position,1}')::double precision + v_dy / v_length * 70.0
                    );
                else
                    v_position := v_planet -> 'position';
                end if;
                v_missions := v_missions || jsonb_build_array(jsonb_build_object(
                    'id', v_mission_id,
                    'owner', p_protector,
                    'origin', p_planet_id,
                    'origin_owned', v_planet -> 'owned',
                    'origin_controlled', v_planet -> 'controlled',
                    'origin_army', v_stationed,
                    'destination', v_home_planet,
                    'send', v_game.current_turn,
                    'travel_turns', 0,
                    'position', v_position,
                    'objective', 'Deploy',
                    'protected_player', null,
                    'return_objective', 'Protect',
                    'army', v_stationed,
                    'bombing', 'None',
                    'combat_probes', false,
                    'jump_gate', false,
                    'logs', '- (' || v_game.current_turn::text || ') Protection access at '
                        || (v_planet ->> 'name') || ' canceled; returning to home planet '
                        || (v_home ->> 'name') || '.'
                ));
            end if;
        end if;
        v_state := jsonb_set(v_state, '{state,missions}', v_missions, false);

        update public.stellarion_turn_submissions submission
           set ready = false,
               submitted_at = clock_timestamp()
         where submission.game_id = p_game_id
           and submission.turn = v_game.current_turn
           and submission.player_id = p_protector
           and submission.ready
           and exists (
               select 1
               from jsonb_array_elements(submission.submission -> 'commands') command
               where command #>> '{SendMission,destination}' = p_planet_id::text
                 and command #>> '{SendMission,objective}' = 'Protect'
           );
        v_withdrew := found;
    end if;

    v_state := jsonb_set(
        v_state,
        array['state', 'map', 'planets', v_planet_index::text],
        v_planet,
        false
    );
    perform public.stellarion_validate_persisted(
        v_state, v_game.max_players, v_game.status, v_game.current_turn
    );
    update public.stellarion_games
       set state = v_state,
           revision = revision + 1,
           updated_at = clock_timestamp()
     where id = p_game_id
     returning revision into v_game.revision;
    if v_withdrew then
        perform public.stellarion_emit_event(
            p_game_id, 'turn_withdrawn', v_game.current_turn, p_protector
        );
    end if;
    perform public.stellarion_emit_event(
        p_game_id, 'protection_changed', v_game.current_turn, p_protector
    );
    return jsonb_build_object(
        'revision', v_game.revision,
        'turn', v_game.current_turn,
        'planet_id', p_planet_id,
        'controller', v_controller,
        'protector', p_protector,
        'allowed', p_allowed
    );
exception
    when invalid_text_representation or numeric_value_out_of_range then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:protection_state';
end;
$$;

create function public.stellarion_start_game(
    p_game_id uuid,
    p_expected_revision bigint,
    p_persisted jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_member public.stellarion_game_players%rowtype;
    v_player_count smallint;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    select * into v_game from public.stellarion_games where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    select * into v_member from public.stellarion_game_players
      where game_id = p_game_id and user_id = auth.uid();
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    select count(*)::smallint into v_player_count
      from public.stellarion_game_players
      where game_id = p_game_id;
    if not v_member.is_creator
       or v_game.status <> 'lobby'
       or v_player_count not between 2 and v_game.max_players then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_STATUS';
    end if;
    if p_expected_revision is null or p_expected_revision < 0 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:revision';
    end if;
    if v_game.revision is distinct from p_expected_revision then
        raise exception using errcode = 'P0001',
            message = 'STLR_CONFLICT:' || p_expected_revision::text || ':' || v_game.revision::text;
    end if;
    perform public.stellarion_validate_persisted(
        p_persisted, v_player_count, 'active', v_game.current_turn
    );
    -- Starting may shrink the generated roster, but not change the chosen rules
    -- or the colors already selected by the lobby's members.
    if (p_persisted #> '{state,rules}') - 'player_count'
           is distinct from (v_game.state #> '{state,rules}') - 'player_count'
       or exists (
           select 1 from public.stellarion_game_players gp
           join lateral jsonb_array_elements(v_game.state #> '{state,players}') old_player
               on (old_player ->> 'id')::bigint = gp.player_id
           join lateral jsonb_array_elements(p_persisted #> '{state,players}') new_player
               on (new_player ->> 'id')::bigint = gp.player_id
           where gp.game_id = p_game_id
             and new_player -> 'color' is distinct from old_player -> 'color'
       ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;

    update public.stellarion_games
       set state = p_persisted,
           max_players = v_player_count,
           status = 'active',
           revision = revision + 1,
           saved_at = clock_timestamp(),
           updated_at = clock_timestamp()
     where id = p_game_id;
    perform public.stellarion_emit_event(p_game_id, 'game_started', null, null);
    return public.stellarion_game_record(p_game_id);
end;
$$;

-- Lobby color selection has its own row-locked operation. A join receives the
-- deterministic color attached to its free player slot. Later claims are
-- serialized on the game row: if two members request the same free color, the
-- first lock holder keeps it and the other receives the unchanged canonical
-- record, restoring that player to the color they had before the request.
create function public.stellarion_set_player_color(
    p_game_id uuid,
    p_color smallint
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_player_id bigint;
    v_old_color jsonb;
    v_new_color jsonb;
    v_players jsonb;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if p_color is null or p_color not between 0 and 5 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:player_color';
    end if;
    select * into v_game from public.stellarion_games where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    select player_id into v_player_id
      from public.stellarion_game_players
      where game_id = p_game_id and user_id = auth.uid();
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if v_game.status <> 'lobby' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_STATUS';
    end if;

    v_new_color := to_jsonb(p_color::integer);
    select player -> 'color' into v_old_color
      from jsonb_array_elements(v_game.state #> '{state,players}') player
      where (player ->> 'id')::bigint = v_player_id;
    if v_old_color is null then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:players';
    end if;
    if v_old_color = v_new_color then
        return public.stellarion_game_record(p_game_id);
    end if;

    -- Only colors held by actual lobby members are unavailable. The complete
    -- generated model also contains future slots, so swap with that unoccupied
    -- slot to preserve the persisted all-player uniqueness invariant.
    if exists (
        select 1 from public.stellarion_game_players gp
        join lateral jsonb_array_elements(v_game.state #> '{state,players}') player
            on (player ->> 'id')::bigint = gp.player_id
        where gp.game_id = p_game_id and gp.player_id <> v_player_id
          and player -> 'color' = v_new_color
    ) then
        return public.stellarion_game_record(p_game_id);
    end if;

    select jsonb_agg(case
        when (player ->> 'id')::bigint = v_player_id
            then jsonb_set(player, '{color}', v_new_color)
        when player -> 'color' = v_new_color
            then jsonb_set(player, '{color}', v_old_color)
        else player end order by ordinal)
      into v_players
      from jsonb_array_elements(v_game.state #> '{state,players}')
           with ordinality as p(player, ordinal);

    update public.stellarion_games
       set state = jsonb_set(v_game.state, '{state,players}', v_players),
           revision = revision + 1,
           saved_at = clock_timestamp(),
           updated_at = clock_timestamp()
     where id = p_game_id;
    perform public.stellarion_emit_event(p_game_id, 'state_changed', null, v_player_id);
    return public.stellarion_game_record(p_game_id);
end;
$$;

-- Manual Save checkpoints the already-authoritative shared snapshot and the
-- caller's current unfinished turn. Other players' local-only choices cannot be
-- observed here; each player transmits their own draft by saving or ending turn.
create function public.stellarion_save_game(
    p_game_id uuid,
    p_expected_revision bigint,
    p_submission jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, extensions, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_player_id bigint;
    v_turn bigint;
    v_generation bigint;
    v_digest text;
    v_existing public.stellarion_turn_submissions%rowtype;
    v_existing_found boolean;
    v_revision bigint;
    v_saved_at timestamptz;
    v_command jsonb;
    v_invitation jsonb;
    v_expected_contributions jsonb;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if p_submission is null or jsonb_typeof(p_submission) is distinct from 'object'
       or pg_column_size(p_submission) > 1048576
       or not (p_submission ?& array['player_id', 'turn', 'generation', 'commands'])
       or p_submission - array['player_id', 'turn', 'generation', 'commands'] <> '{}'::jsonb
       or jsonb_typeof(p_submission -> 'commands') is distinct from 'array'
       or (case
           when jsonb_typeof(p_submission -> 'commands') = 'array'
               then jsonb_array_length(p_submission -> 'commands') > 1024
           else true
       end) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:submission';
    end if;
    begin
        v_player_id := (p_submission ->> 'player_id')::bigint;
        v_turn := (p_submission ->> 'turn')::bigint;
        v_generation := (p_submission ->> 'generation')::bigint;
    exception
        when invalid_text_representation or numeric_value_out_of_range then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:submission_ids';
    end;
    if v_player_id is null or v_player_id not between 1 and 4
       or v_turn is null or v_turn < 1 or v_generation is null or v_generation < 0 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:submission_ids';
    end if;
    select * into v_game from public.stellarion_games where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if not exists (
        select 1 from public.stellarion_game_players
        where game_id = p_game_id and user_id = auth.uid() and player_id = v_player_id
    ) or not exists (
        select 1 from jsonb_array_elements(v_game.state #> '{state,players}') as players(player)
        where (player ->> 'id')::bigint = v_player_id
          and (player ->> 'spectator')::boolean = false
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if v_game.status <> 'active' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_STATUS';
    end if;
    if v_turn is distinct from v_game.current_turn then
        raise exception using errcode = 'P0001',
            message = 'STLR_STALE_SUBMISSION:' || v_game.current_turn::text || ':' || v_turn::text;
    end if;
    if p_expected_revision is null or p_expected_revision < 0 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:revision';
    end if;
    if v_game.revision is distinct from p_expected_revision then
        raise exception using errcode = 'P0001',
            message = 'STLR_CONFLICT:' || p_expected_revision::text || ':' || v_game.revision::text;
    end if;

    for v_command in
        select value from jsonb_array_elements(p_submission -> 'commands')
         where value ->> 'kind' = 'send_joint_mission'
    loop
        select invitation into v_invitation
          from public.stellarion_joint_attacks
         where game_id = p_game_id
           and attack_id = (v_command ->> 'attack_id')::bigint;
        if not found then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:joint_attack';
        end if;
        select jsonb_agg(participant -> 'contribution' order by ordinal)
          into v_expected_contributions
          from jsonb_array_elements(v_invitation -> 'participants')
               with ordinality as entries(participant, ordinal)
         where participant ->> 'response' = 'accepted';
        if coalesce((v_invitation ->> 'canceled')::boolean, false)
           or (v_invitation ->> 'turn')::bigint <> v_turn
           or (v_invitation ->> 'inviter')::bigint <> v_player_id
           or (v_invitation ->> 'destination')::bigint <> (v_command ->> 'destination')::bigint
           or v_invitation -> 'objective' <> v_command -> 'objective'
           or v_invitation -> 'bombing' <> v_command -> 'bombing'
           or v_invitation -> 'combat_probes' <> v_command -> 'combat_probes'
           or exists (
               select 1 from jsonb_array_elements(v_invitation -> 'participants') participant
                where participant ->> 'response' = 'pending'
           )
           or jsonb_array_length(v_expected_contributions) < 2
           or v_expected_contributions <> v_command -> 'contributions'
        then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:joint_attack';
        end if;
    end loop;

    v_digest := encode(
        digest(
            convert_to('stellarion-turn-submission-v1' || p_submission::text, 'UTF8'),
            'sha256'
        ),
        'hex'
    );
    select * into v_existing
      from public.stellarion_turn_submissions
      where game_id = p_game_id and turn = v_turn and player_id = v_player_id;
    v_existing_found := found;
    if v_existing_found then
        if (v_existing.submission ->> 'generation')::bigint <> v_generation then
            raise exception using errcode = 'P0001',
                message = 'STLR_DUPLICATE_SUBMISSION:' || v_player_id::text || ':' || v_turn::text;
        end if;
        if v_existing.ready and v_existing.digest <> v_digest then
            raise exception using errcode = 'P0001', message = 'STLR_TURN_COMMITTED';
        end if;
    elsif v_generation <> 0 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:readiness_generation';
    end if;
    if not v_existing_found or not v_existing.ready then
        insert into public.stellarion_turn_submissions (
            game_id, turn, player_id, submission, digest, ready
        ) values (
            p_game_id, v_turn, v_player_id, p_submission, v_digest, false
        ) on conflict (game_id, turn, player_id) do update
            set submission = excluded.submission,
                digest = excluded.digest,
                ready = false,
                submitted_at = clock_timestamp();
    end if;

    -- The matching revision proves that this client already has the canonical
    -- snapshot. Persist only this caller's unfinished commands and renew the
    -- retention checkpoint. Draft saves deliberately do not advance the shared
    -- revision or emit an event, so players can save concurrently without
    -- invalidating each other's canonical snapshot.
    update public.stellarion_games
       set saved_at = clock_timestamp(),
           updated_at = clock_timestamp()
     where id = p_game_id
     returning revision, saved_at into v_revision, v_saved_at;
    return jsonb_build_object(
        'revision', v_revision,
        'saved_at', floor(extract(epoch from v_saved_at))::bigint
    );
end;
$$;

-- End turn marks a draft ready. It remains reversible until every active player
-- is ready. Readiness and withdrawal take the game row lock, so the final ready
-- freezes the batch atomically. Generations reject delayed requests after editing.
create function public.stellarion_submit_turn(
    p_game_id uuid,
    p_submission jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, extensions, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_player_id bigint;
    v_turn bigint;
    v_digest text;
    v_existing public.stellarion_turn_submissions%rowtype;
    v_generation bigint;
    v_command jsonb;
    v_invitation jsonb;
    v_expected_contributions jsonb;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if p_submission is null or jsonb_typeof(p_submission) is distinct from 'object'
       or pg_column_size(p_submission) > 1048576
       or not (p_submission ?& array['player_id', 'turn', 'generation', 'commands'])
       or p_submission - array['player_id', 'turn', 'generation', 'commands'] <> '{}'::jsonb
       or jsonb_typeof(p_submission -> 'commands') is distinct from 'array'
       or (case
           when jsonb_typeof(p_submission -> 'commands') = 'array'
               then jsonb_array_length(p_submission -> 'commands') > 1024
           else true
       end) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:submission';
    end if;
    begin
        v_player_id := (p_submission ->> 'player_id')::bigint;
        v_turn := (p_submission ->> 'turn')::bigint;
        v_generation := (p_submission ->> 'generation')::bigint;
    exception
        when invalid_text_representation or numeric_value_out_of_range then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:submission_ids';
    end;
    if v_player_id is null or v_player_id not between 1 and 4
       or v_turn is null or v_turn < 1 or v_generation is null or v_generation < 0 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:submission_ids';
    end if;

    select * into v_game from public.stellarion_games where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if v_game.status <> 'active' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_STATUS';
    end if;
    if not exists (
        select 1 from public.stellarion_game_players
        where game_id = p_game_id
          and user_id = auth.uid()
          and player_id = v_player_id
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if not exists (
        select 1
        from jsonb_array_elements(v_game.state #> '{state,players}') as players(player)
        where (player ->> 'id')::bigint = v_player_id
          and (player ->> 'spectator')::boolean = false
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if v_turn is distinct from v_game.current_turn then
        raise exception using errcode = 'P0001',
            message = 'STLR_STALE_SUBMISSION:' || v_game.current_turn::text || ':' || v_turn::text;
    end if;

    for v_command in
        select value from jsonb_array_elements(p_submission -> 'commands')
         where value ->> 'kind' = 'send_joint_mission'
    loop
        select invitation into v_invitation
          from public.stellarion_joint_attacks
         where game_id = p_game_id
           and attack_id = (v_command ->> 'attack_id')::bigint;
        if not found then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:joint_attack';
        end if;
        select jsonb_agg(participant -> 'contribution' order by ordinal)
          into v_expected_contributions
          from jsonb_array_elements(v_invitation -> 'participants')
               with ordinality as entries(participant, ordinal)
         where participant ->> 'response' = 'accepted';
        if coalesce((v_invitation ->> 'canceled')::boolean, false)
           or (v_invitation ->> 'turn')::bigint <> v_turn
           or (v_invitation ->> 'inviter')::bigint <> v_player_id
           or (v_invitation ->> 'destination')::bigint <> (v_command ->> 'destination')::bigint
           or v_invitation -> 'objective' <> v_command -> 'objective'
           or v_invitation -> 'bombing' <> v_command -> 'bombing'
           or v_invitation -> 'combat_probes' <> v_command -> 'combat_probes'
           or exists (
               select 1 from jsonb_array_elements(v_invitation -> 'participants') participant
                where participant ->> 'response' = 'pending'
           )
           or jsonb_array_length(v_expected_contributions) < 2
           or v_expected_contributions <> v_command -> 'contributions'
        then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:joint_attack';
        end if;
    end loop;

    p_submission := jsonb_set(p_submission, '{generation}', to_jsonb(v_generation));
    v_digest := encode(
        digest(
            convert_to('stellarion-turn-submission-v1' || p_submission::text, 'UTF8'),
            'sha256'
        ),
        'hex'
    );
    select * into v_existing
      from public.stellarion_turn_submissions
      where game_id = p_game_id and turn = v_turn and player_id = v_player_id;
    if found then
        if (v_existing.submission ->> 'generation')::bigint <> v_generation
           or (v_existing.ready and v_existing.digest <> v_digest) then
            raise exception using errcode = 'P0001',
                message = 'STLR_DUPLICATE_SUBMISSION:' || v_player_id::text || ':' || v_turn::text;
        end if;
        if v_existing.ready then
            return jsonb_build_object('disposition', 'duplicate');
        end if;
    elsif v_generation <> 0 then
        raise exception using errcode = 'P0001',
            message = 'STLR_INVALID_DATA:readiness_generation';
    end if;

    insert into public.stellarion_turn_submissions (
        game_id, turn, player_id, submission, digest
    ) values (
        p_game_id, v_turn, v_player_id, p_submission, v_digest
    ) on conflict (game_id, turn, player_id) do update
        set submission = excluded.submission, digest = excluded.digest,
            ready = true, submitted_at = clock_timestamp();
    perform public.stellarion_emit_event(
        p_game_id, 'turn_submitted', v_turn, v_player_id
    );
    return jsonb_build_object('disposition', 'inserted');
end;
$$;

-- Retain withdrawn orders as a draft. Besides preserving them across reconnects,
-- this prevents a timed-out ready request from arriving late and making the player
-- ready again. An identical withdrawal retry returns the same draft.
create function public.stellarion_withdraw_turn(
    p_game_id uuid,
    p_turn bigint,
    p_generation bigint
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, extensions, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_player_id bigint;
    v_existing public.stellarion_turn_submissions%rowtype;
    v_draft jsonb;
    v_digest text;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if p_turn is null or p_turn < 1 or p_generation is null or p_generation < 0 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:readiness';
    end if;
    select * into v_game from public.stellarion_games where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    select player_id into v_player_id from public.stellarion_game_players
     where game_id = p_game_id and user_id = auth.uid();
    if not found or not exists (
        select 1 from jsonb_array_elements(v_game.state #> '{state,players}') as players(player)
        where (player ->> 'id')::bigint = v_player_id
          and (player ->> 'spectator')::boolean = false
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if v_game.status <> 'active' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_STATUS';
    end if;
    if p_turn <> v_game.current_turn then
        raise exception using errcode = 'P0001',
            message = 'STLR_STALE_SUBMISSION:' || v_game.current_turn::text || ':' || p_turn::text;
    end if;
    select * into v_existing from public.stellarion_turn_submissions
     where game_id = p_game_id and turn = p_turn and player_id = v_player_id;
    if found then
        if not v_existing.ready
           and p_generation <= (v_existing.submission ->> 'generation')::bigint then
            return v_existing.submission;
        end if;
        if p_generation <> (v_existing.submission ->> 'generation')::bigint then
            raise exception using errcode = 'P0001',
                message = 'STLR_DUPLICATE_SUBMISSION:' || v_player_id::text || ':' || p_turn::text;
        end if;
        v_draft := v_existing.submission;
    else
        if p_generation <> 0 then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:readiness_generation';
        end if;
        v_draft := jsonb_build_object('player_id', v_player_id, 'turn', p_turn, 'commands', '[]'::jsonb);
    end if;
    if not exists (
        select 1 from jsonb_array_elements(v_game.state #> '{state,players}') as players(player)
        where (player ->> 'spectator')::boolean = false
          and not exists (
              select 1 from public.stellarion_turn_submissions s
              where s.game_id = p_game_id and s.turn = p_turn
                and s.player_id = (player ->> 'id')::bigint and s.ready
          )
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_TURN_COMMITTED';
    end if;
    if p_generation = 9223372036854775807 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:readiness_generation';
    end if;
    v_draft := jsonb_set(v_draft, '{generation}', to_jsonb(p_generation + 1));
    v_digest := encode(digest(convert_to('stellarion-turn-submission-v1' || v_draft::text, 'UTF8'), 'sha256'), 'hex');
    insert into public.stellarion_turn_submissions (game_id, turn, player_id, submission, digest, ready)
    values (p_game_id, p_turn, v_player_id, v_draft, v_digest, false)
    on conflict (game_id, turn, player_id) do update
        set submission = excluded.submission, digest = excluded.digest, ready = false;
    perform public.stellarion_emit_event(p_game_id, 'turn_withdrawn', p_turn, v_player_id);
    return v_draft;
end;
$$;

create function public.stellarion_load_turn_submissions(
    p_game_id uuid,
    p_turn bigint
)
returns jsonb
language plpgsql
stable
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_result jsonb;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if p_turn is null or p_turn < 1 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:turn';
    end if;
    if not exists (select 1 from public.stellarion_games where id = p_game_id) then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if not exists (
        select 1 from public.stellarion_game_players
        where game_id = p_game_id and user_id = auth.uid()
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;

    select coalesce(
               jsonb_agg(
                   jsonb_build_object(
                       'submission', submission,
                       'digest', digest,
                       'ready', ready
                   ) order by player_id
               ),
               '[]'::jsonb
           )
      into v_result
      from public.stellarion_turn_submissions
      where game_id = p_game_id and turn = p_turn;
    return v_result;
end;
$$;

create function public.stellarion_publish_resolution(
    p_game_id uuid,
    p_expected_revision bigint,
    p_resolved_turn bigint,
    p_persisted jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_next_status text;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    select * into v_game from public.stellarion_games where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if not exists (
        select 1 from public.stellarion_game_players
        where game_id = p_game_id and user_id = auth.uid()
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if p_expected_revision is null or p_expected_revision < 0 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:revision';
    end if;
    if v_game.revision is distinct from p_expected_revision then
        raise exception using errcode = 'P0001',
            message = 'STLR_CONFLICT:' || p_expected_revision::text || ':' || v_game.revision::text;
    end if;
    if p_resolved_turn is null
       or v_game.status <> 'active'
       or v_game.current_turn is distinct from p_resolved_turn then
        raise exception using errcode = 'P0001',
            message = 'STLR_STALE_SUBMISSION:' || v_game.current_turn::text || ':' || p_resolved_turn::text;
    end if;

    v_next_status := p_persisted #>> '{state,status}';
    if v_next_status is null or v_next_status not in ('active', 'finished') then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_STATUS';
    end if;
    perform public.stellarion_validate_persisted(
        p_persisted,
        v_game.max_players,
        v_next_status,
        p_resolved_turn + 1
    );
    if p_persisted #> '{state,rules}' is distinct from v_game.state #> '{state,rules}'
       or p_persisted #> '{state,rng,seed}' is distinct from v_game.state #> '{state,rng,seed}' then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;

    if exists (
        select 1
        from jsonb_array_elements(v_game.state #> '{state,players}') as players(player)
        where (player ->> 'spectator')::boolean = false
          and not exists (
              select 1
              from public.stellarion_turn_submissions as submission
              where submission.game_id = p_game_id
                and submission.turn = p_resolved_turn
                and submission.player_id = (player ->> 'id')::bigint
                and submission.ready
          )
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_TURN_INCOMPLETE';
    end if;

    update public.stellarion_games
       set state = p_persisted,
           status = v_next_status,
           current_turn = p_resolved_turn + 1,
           revision = revision + 1,
           saved_at = clock_timestamp(),
           updated_at = clock_timestamp()
     where id = p_game_id;

    perform public.stellarion_emit_event(
        p_game_id,
        case when v_next_status = 'finished' then 'game_finished' else 'turn_resolved' end,
        p_resolved_turn + 1,
        null
    );

    -- Retain a short diagnostic/idempotency window without unbounded rows.
    delete from public.stellarion_turn_submissions
     where game_id = p_game_id and turn < p_resolved_turn - 8;
    delete from public.stellarion_joint_attacks
     where game_id = p_game_id and turn <= p_resolved_turn;
    delete from public.stellarion_trades
     where game_id = p_game_id and turn <= p_resolved_turn;

    return public.stellarion_game_record(p_game_id);
end;
$$;

create function public.stellarion_events_since(
    p_game_id uuid,
    p_after_sequence bigint
)
returns jsonb
language plpgsql
stable
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_events jsonb;
    v_cursor bigint;
    v_player bigint;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if not exists (select 1 from public.stellarion_games where id = p_game_id) then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    select player_id into v_player
      from public.stellarion_game_players
     where game_id = p_game_id and user_id = auth.uid();
    if v_player is null then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if p_after_sequence is null or p_after_sequence < 0 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:event_cursor';
    end if;

    with raw_replay as (
        select event.sequence,
               event.game_id,
               event.kind,
               event.revision,
               event.turn,
               event.player_id
        from public.stellarion_game_events as event
        where event.game_id = p_game_id
          and event.sequence > p_after_sequence
        order by event.sequence
        limit 256
    ), replay as (
        select * from raw_replay
         where kind not in ('joint_attack_changed', 'trade_changed') or player_id = v_player
    )
    select coalesce(
               jsonb_agg(
                   jsonb_build_object(
                       'sequence', replay.sequence,
                       'game_id', replay.game_id::text,
                       'kind', replay.kind,
                       'revision', replay.revision,
                       'turn', replay.turn,
                       'player_id', replay.player_id
                   ) order by replay.sequence
               ),
               '[]'::jsonb
           ),
           coalesce((select max(raw_replay.sequence) from raw_replay), p_after_sequence)
      into v_events, v_cursor
      from replay;

    return jsonb_build_object('events', v_events, 'cursor', v_cursor);
end;
$$;

create function public.stellarion_resume_game(p_game_id uuid)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;

    select * into v_game
      from public.stellarion_games
      where id = p_game_id
      for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    if not exists (
        select 1 from public.stellarion_game_players
        where game_id = p_game_id and user_id = auth.uid() and is_creator
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if v_game.status <> 'active'
       or exists (
           select 1 from public.stellarion_game_players
           where game_id = p_game_id
             and not public.stellarion_connection_is_live(connected, last_seen_at)
       ) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_STATUS';
    end if;

    perform public.stellarion_emit_event(p_game_id, 'game_resumed', v_game.current_turn, null);
    return jsonb_build_object('ok', true);
end;
$$;

create function public.stellarion_set_connected(
    p_game_id uuid,
    p_connected boolean
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, auth
as $$
declare
    v_game public.stellarion_games%rowtype;
    v_player public.stellarion_game_players%rowtype;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if p_connected is null then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:connected';
    end if;
    -- Serialize departure with joins and starting the match. All these RPCs
    -- lock the game before its memberships, so a started game cannot be deleted.
    select * into v_game from public.stellarion_games
      where id = p_game_id for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
    end if;
    select * into v_player
      from public.stellarion_game_players
      where game_id = p_game_id and user_id = auth.uid()
      for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;

    if not p_connected and v_game.status = 'lobby' and v_player.is_creator then
        -- Cascades erase every membership/recovery code, submission, and event.
        -- No tombstone is retained: guest event polls report GAME_NOT_FOUND and
        -- return those clients to the menu, even if a Realtime hint was missed.
        delete from public.stellarion_games where id = p_game_id;
        return jsonb_build_object('ok', true, 'members', '[]'::jsonb);
    end if;

    update public.stellarion_game_players
       set connected = p_connected,
           last_seen_at = clock_timestamp()
     where game_id = p_game_id and player_id = v_player.player_id;

    if public.stellarion_connection_is_live(v_player.connected, v_player.last_seen_at)
       is distinct from p_connected then
        perform public.stellarion_emit_event(
            p_game_id,
            case when p_connected then 'player_connected' else 'player_disconnected' end,
            null,
            v_player.player_id
        );
    end if;
    return jsonb_build_object(
        'ok', true,
        'members', public.stellarion_membership_records(p_game_id)
    );
end;
$$;

-- Tables are RPC-only except for the Realtime event stream. Recovery codes are
-- returned only by caller-specific membership and resume RPC responses.
revoke all on table public.stellarion_games from anon, authenticated;
revoke all on table public.stellarion_game_players from anon, authenticated;
revoke all on table public.stellarion_turn_submissions from anon, authenticated;
revoke all on table public.stellarion_joint_attacks from anon, authenticated;
revoke all on table public.stellarion_trades from anon, authenticated;
revoke all on table public.stellarion_game_events from anon, authenticated;
grant select on table public.stellarion_game_events to authenticated;

revoke all on function public.stellarion_is_game_member(uuid) from public, anon;
grant execute on function public.stellarion_is_game_member(uuid) to authenticated;
revoke all on function public.stellarion_can_read_event(uuid, text, bigint) from public, anon;
grant execute on function public.stellarion_can_read_event(uuid, text, bigint) to authenticated;

revoke all on function public.stellarion_validate_persisted(jsonb, smallint, text, bigint)
    from public, anon, authenticated;
revoke all on function public.stellarion_stamp_finished_game()
    from public, anon, authenticated;
revoke all on function public.stellarion_connection_is_live(boolean, timestamptz)
    from public, anon, authenticated;
revoke all on function public.stellarion_membership_records(uuid)
    from public, anon, authenticated;
revoke all on function public.stellarion_game_record(uuid)
    from public, anon, authenticated;
revoke all on function public.stellarion_membership_record(uuid, uuid)
    from public, anon, authenticated;
revoke all on function public.stellarion_emit_event(uuid, text, bigint, bigint)
    from public, anon, authenticated;

revoke all on function public.stellarion_create_game(text, text, text, smallint, jsonb)
    from public, anon;
grant execute on function public.stellarion_create_game(text, text, text, smallint, jsonb)
    to authenticated;
revoke all on function public.stellarion_join_game(text, text, text)
    from public, anon;
revoke all on function public.stellarion_recover_player(text, text)
    from public, anon;
revoke all on function public.stellarion_list_games()
    from public, anon;
revoke all on function public.stellarion_load_game(uuid)
    from public, anon;
revoke all on function public.stellarion_set_protection_permission(uuid, bigint, bigint, boolean)
    from public, anon;
grant execute on function public.stellarion_set_protection_permission(uuid, bigint, bigint, boolean)
    to authenticated;
revoke all on function public.stellarion_create_joint_attack(uuid, jsonb)
    from public, anon;
grant execute on function public.stellarion_create_joint_attack(uuid, jsonb)
    to authenticated;
revoke all on function public.stellarion_respond_joint_attack(uuid, bigint, text, jsonb)
    from public, anon;
grant execute on function public.stellarion_respond_joint_attack(uuid, bigint, text, jsonb)
    to authenticated;
revoke all on function public.stellarion_cancel_joint_attack(uuid, bigint)
    from public, anon;
grant execute on function public.stellarion_cancel_joint_attack(uuid, bigint)
    to authenticated;
revoke all on function public.stellarion_load_joint_attacks(uuid)
    from public, anon;
grant execute on function public.stellarion_load_joint_attacks(uuid)
    to authenticated;
revoke all on function public.stellarion_trade_resources_valid(jsonb, boolean)
    from public, anon, authenticated;
revoke all on function public.stellarion_trade_capacity(jsonb, bigint)
    from public, anon, authenticated;
revoke all on function public.stellarion_trade_route_valid(jsonb, jsonb)
    from public, anon, authenticated;
revoke all on function public.stellarion_create_trade(uuid, jsonb)
    from public, anon;
grant execute on function public.stellarion_create_trade(uuid, jsonb)
    to authenticated;
revoke all on function public.stellarion_respond_trade(uuid, bigint, jsonb, text)
    from public, anon;
grant execute on function public.stellarion_respond_trade(uuid, bigint, jsonb, text)
    to authenticated;
revoke all on function public.stellarion_load_trades(uuid)
    from public, anon;
grant execute on function public.stellarion_load_trades(uuid)
    to authenticated;
revoke all on function public.stellarion_start_game(uuid, bigint, jsonb)
    from public, anon;
grant execute on function public.stellarion_start_game(uuid, bigint, jsonb)
    to authenticated;
revoke all on function public.stellarion_resume_game(uuid)
    from public, anon;
revoke all on function public.stellarion_set_player_color(uuid, smallint)
    from public, anon;
grant execute on function public.stellarion_set_player_color(uuid, smallint)
    to authenticated;
revoke all on function public.stellarion_save_game(uuid, bigint, jsonb)
    from public, anon;
grant execute on function public.stellarion_save_game(uuid, bigint, jsonb)
    to authenticated;
revoke all on function public.stellarion_submit_turn(uuid, jsonb)
    from public, anon;
grant execute on function public.stellarion_submit_turn(uuid, jsonb)
    to authenticated;
revoke all on function public.stellarion_withdraw_turn(uuid, bigint, bigint)
    from public, anon, authenticated;
grant execute on function public.stellarion_withdraw_turn(uuid, bigint, bigint)
    to authenticated;
revoke all on function public.stellarion_load_turn_submissions(uuid, bigint)
    from public, anon;
revoke all on function public.stellarion_publish_resolution(uuid, bigint, bigint, jsonb)
    from public, anon;
grant execute on function public.stellarion_publish_resolution(uuid, bigint, bigint, jsonb)
    to authenticated;
revoke all on function public.stellarion_events_since(uuid, bigint)
    from public, anon;
revoke all on function public.stellarion_set_connected(uuid, boolean)
    from public, anon;

grant execute on function public.stellarion_join_game(text, text, text)
    to authenticated;
grant execute on function public.stellarion_recover_player(text, text)
    to authenticated;
grant execute on function public.stellarion_list_games()
    to authenticated;
grant execute on function public.stellarion_load_game(uuid)
    to authenticated;
grant execute on function public.stellarion_resume_game(uuid)
    to authenticated;
grant execute on function public.stellarion_load_turn_submissions(uuid, bigint)
    to authenticated;
grant execute on function public.stellarion_events_since(uuid, bigint)
    to authenticated;
grant execute on function public.stellarion_set_connected(uuid, boolean)
    to authenticated;

-- Supabase Realtime publishes only semantic/durable events. The game snapshot
-- and command rows remain RPC-only and are never streamed for local rendering.
alter table public.stellarion_game_events replica identity full;
do $$
begin
    if not exists (
        select 1 from pg_publication where pubname = 'supabase_realtime'
    ) then
        create publication supabase_realtime;
    end if;
    if not exists (
        select 1
        from pg_publication_tables
        where pubname = 'supabase_realtime'
          and schemaname = 'public'
          and tablename = 'stellarion_game_events'
    ) then
        alter publication supabase_realtime
            add table public.stellarion_game_events;
    end if;
end;
$$;

-- Delete expired games even when no client opens the resume overview.
-- Run every minute: any game expires 30 days after its last save; finished
-- games also expire 48 hours after completion, whichever deadline comes first.
-- Foreign keys also delete their players, recovery codes, turns, and events.
create function public.stellarion_delete_expired_games()
returns bigint
language sql
set search_path = pg_catalog, public
as $$
    with deleted as (
        delete from public.stellarion_games
         where saved_at <= statement_timestamp() - interval '30 days'
            or (status = 'finished'
                and finished_at <= statement_timestamp() - interval '48 hours')
        returning id
    )
    select count(*) from deleted;
$$;

-- Only the database owner running the scheduled job may invoke cleanup.
revoke all on function public.stellarion_delete_expired_games()
    from public, anon, authenticated;

select cron.schedule(
    'stellarion-delete-expired-games',
    '* * * * *',
    'select public.stellarion_delete_expired_games();'
);

-- Ensure the Data API sees every RPC immediately after this fresh-project install.
notify pgrst, 'reload schema';

commit;
