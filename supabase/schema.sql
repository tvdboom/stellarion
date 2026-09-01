-- Complete destructive reset and install for Stellarion multiplayer.
-- Running this file removes every existing Stellarion game, player, turn,
-- event, policy, and RPC before recreating the current database contract.

begin;

-- Supabase system schemas (auth, storage, extensions, and Realtime) stay intact;
-- the complete application-facing database is reset in one operation.
drop schema if exists public cascade;
create schema public;
grant usage on schema public to postgres, anon, authenticated;
grant all on schema public to postgres;

create schema if not exists extensions;
create extension if not exists pgcrypto with schema extensions;

-- Authenticated clients must never be able to create shadow objects used by
-- SECURITY DEFINER functions.
revoke create on schema public from public;

create table public.stellarion_games (
    id uuid primary key default gen_random_uuid(),
    code text not null unique,
    created_by uuid not null references auth.users(id) on delete restrict,
    max_players smallint not null,
    status text not null,
    persisted_schema_version integer not null,
    state jsonb not null,
    revision bigint not null default 0,
    current_turn bigint not null,
    event_sequence bigint not null default 0,
    created_at timestamptz not null default clock_timestamp(),
    updated_at timestamptz not null default clock_timestamp(),
    constraint stellarion_games_code_format check (code ~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{6}$'),
    constraint stellarion_games_player_count check (max_players between 2 and 4),
    constraint stellarion_games_status check (status in ('lobby', 'active', 'finished')),
    constraint stellarion_games_schema_version check (persisted_schema_version > 0),
    constraint stellarion_games_revision check (revision >= 0),
    constraint stellarion_games_turn check (current_turn >= 1),
    constraint stellarion_games_event_sequence check (event_sequence >= 0),
    constraint stellarion_games_state_object check (jsonb_typeof(state) = 'object')
);

create table public.stellarion_game_players (
    game_id uuid not null references public.stellarion_games(id) on delete cascade,
    player_id bigint not null,
    user_id uuid not null references auth.users(id) on delete restrict,
    display_name text not null,
    recovery_hash text not null,
    is_creator boolean not null default false,
    identity_version bigint not null default 1,
    connected boolean not null default false,
    joined_at timestamptz not null default clock_timestamp(),
    last_seen_at timestamptz not null default clock_timestamp(),
    primary key (game_id, player_id),
    constraint stellarion_game_players_user unique (game_id, user_id),
    constraint stellarion_game_players_recovery unique (game_id, recovery_hash),
    constraint stellarion_game_players_slot check (player_id between 1 and 4),
    constraint stellarion_game_players_name check (
        char_length(btrim(display_name)) between 1 and 32
        and display_name = btrim(display_name)
    ),
    constraint stellarion_game_players_recovery_hash check (recovery_hash ~ '^[0-9a-f]{64}$'),
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

alter table public.stellarion_games enable row level security;
alter table public.stellarion_game_players enable row level security;
alter table public.stellarion_turn_submissions enable row level security;
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
    using (public.stellarion_is_game_member(game_id));

-- Validates the database-visible invariants of a versioned Rust snapshot.
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
    v_schema integer;
    v_player_count integer;
    v_turn bigint;
    v_status text;
    v_players jsonb;
    v_total integer;
    v_unique integer;
    v_min_id bigint;
    v_max_id bigint;
    v_planets_per_player integer;
    v_colonizable_percent integer;
    v_moons_percent integer;
    v_planets jsonb;
    v_missions jsonb;
    v_planet_total integer;
    v_unique_planets integer;
    v_min_planet_id bigint;
    v_max_planet_id bigint;
begin
    if p_persisted is null
       or jsonb_typeof(p_persisted) is distinct from 'object'
       or pg_column_size(p_persisted) > 67108864
       or jsonb_typeof(p_persisted -> 'state') is distinct from 'object' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:persisted object';
    end if;

    v_schema := (p_persisted ->> 'schema_version')::integer;
    v_player_count := (p_persisted #>> '{state,rules,player_count}')::integer;
    v_turn := (p_persisted #>> '{state,turn}')::bigint;
    v_status := p_persisted #>> '{state,status}';
    v_players := p_persisted #> '{state,players}';
    v_planets_per_player := (p_persisted #>> '{state,rules,planets_per_player}')::integer;
    v_colonizable_percent := (p_persisted #>> '{state,rules,colonizable_percent}')::integer;
    v_moons_percent := (p_persisted #>> '{state,rules,moons_percent}')::integer;
    v_planets := p_persisted #> '{state,map,planets}';
    v_missions := p_persisted #> '{state,missions}';

    if v_schema is distinct from 1 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:schema_version';
    end if;
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
    if v_planets_per_player is null or v_planets_per_player not between 5 and 20
       or v_colonizable_percent is null or v_colonizable_percent not between 1 and 100
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

    if exists (
        select 1
        from jsonb_array_elements(v_players) as entries(entry)
        where case
            when jsonb_typeof(entry) is distinct from 'object' then true
            when jsonb_typeof(entry -> 'reports') is distinct from 'array' then true
            else jsonb_array_length(entry -> 'reports') > 512
        end
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:reports';
    end if;

    select count(*),
           count(distinct (entry ->> 'id')::bigint),
           min((entry ->> 'id')::bigint),
           max((entry ->> 'id')::bigint)
      into v_total, v_unique, v_min_id, v_max_id
      from jsonb_array_elements(v_players) as entries(entry);

    if v_total <> p_max_players
       or v_unique <> p_max_players
       or v_min_id <> 1
       or v_max_id <> p_max_players then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:player_ids';
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
               'max_players', g.max_players,
               'status', g.status,
               'persisted', g.state,
               'members', coalesce(
                   (
                       select jsonb_agg(
                           jsonb_build_object(
                               'game_id', gp.game_id::text,
                               'player_id', gp.player_id,
                               'user_id', gp.user_id::text,
                               'display_name', gp.display_name,
                               'is_creator', gp.is_creator,
                               'identity_version', gp.identity_version,
                               'connected', gp.connected
                           ) order by gp.player_id
                       )
                       from public.stellarion_game_players as gp
                       where gp.game_id = g.id
                   ),
                   '[]'::jsonb
               )
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
               'connected', gp.connected
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

    -- Realtime is a wake-up path; persisted state is always reloaded. Keeping
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
    p_recovery_hash text,
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
    v_hash text := lower(p_recovery_hash);
begin
    if v_user_id is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if v_code is null or v_code !~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{6}$' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:game_code';
    end if;
    if v_name is null or char_length(v_name) not between 1 and 32 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:display_name';
    end if;
    if v_hash is null or v_hash !~ '^[0-9a-f]{64}$' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:recovery_hash';
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
            persisted_schema_version,
            state,
            current_turn
        ) values (
            v_code,
            v_user_id,
            p_max_players,
            'lobby',
            (p_persisted ->> 'schema_version')::integer,
            p_persisted,
            (p_persisted #>> '{state,turn}')::bigint
        )
        returning id into v_game_id;
    exception
        when unique_violation then
            raise exception using errcode = 'P0001', message = 'STLR_CODE_COLLISION';
    end;

    insert into public.stellarion_game_players (
        game_id, player_id, user_id, display_name, recovery_hash, is_creator
    ) values (
        v_game_id, 1, v_user_id, v_name, v_hash, true
    );

    perform public.stellarion_emit_event(v_game_id, 'player_joined', null, 1);

    return jsonb_build_object(
        'game', public.stellarion_game_record(v_game_id),
        'membership', public.stellarion_membership_record(v_game_id, v_user_id),
        'disposition', 'joined'
    );
end;
$$;

create function public.stellarion_join_game(
    p_code text,
    p_display_name text,
    p_recovery_hash text
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
    v_hash text := lower(p_recovery_hash);
begin
    if v_user_id is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if v_code is null or v_code !~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{6}$'
       or v_name is null or char_length(v_name) not between 1 and 32
       or v_hash is null or v_hash !~ '^[0-9a-f]{64}$' then
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
            game_id, player_id, user_id, display_name, recovery_hash
        ) values (
            v_game.id, v_player_id, v_user_id, v_name, v_hash
        );
    exception
        when unique_violation then
            -- The game row lock serializes slot claims. A remaining violation
            -- means the caller reused a recovery hash or identity unexpectedly.
            raise exception using errcode = 'P0001', message = 'STLR_ALREADY_MEMBER';
    end;

    perform public.stellarion_emit_event(v_game.id, 'player_joined', null, v_player_id);
    return jsonb_build_object(
        'game', public.stellarion_game_record(v_game.id),
        'membership', public.stellarion_membership_record(v_game.id, v_user_id),
        'disposition', 'joined'
    );
end;
$$;

create function public.stellarion_recover_player(
    p_code text,
    p_recovery_hash text,
    p_replacement_recovery_hash text
)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, public, extensions, auth
as $$
declare
    v_user_id uuid := auth.uid();
    v_game_id uuid;
    v_player_id bigint;
    v_code text := upper(btrim(p_code));
    v_hash text := lower(p_recovery_hash);
    v_replacement text := lower(p_replacement_recovery_hash);
begin
    if v_user_id is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if v_code is null or v_code !~ '^[0-9ABCDEFGHJKMNPQRSTVWXYZ]{6}$'
       or v_hash is null or v_hash !~ '^[0-9a-f]{64}$'
       or v_replacement is null or v_replacement !~ '^[0-9a-f]{64}$' then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:recovery_hash';
    end if;
    if v_hash = v_replacement then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:recovery_rotation';
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

    select player_id into v_player_id
      from public.stellarion_game_players
      where game_id = v_game_id and recovery_hash = v_hash
      for update;
    if not found then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_RECOVERY';
    end if;

    begin
        update public.stellarion_game_players
           set user_id = v_user_id,
               recovery_hash = v_replacement,
               identity_version = identity_version + 1,
               connected = false,
               last_seen_at = clock_timestamp()
         where game_id = v_game_id and player_id = v_player_id;
    exception
        when unique_violation then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_RECOVERY';
    end;

    perform public.stellarion_emit_event(v_game_id, 'player_recovered', null, v_player_id);
    return jsonb_build_object(
        'game', public.stellarion_game_record(v_game_id),
        'membership', public.stellarion_membership_record(v_game_id, v_user_id),
        'disposition', 'reconnected'
    );
end;
$$;

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
                        'status', g.status,
                        'turn', g.current_turn,
                        'player_id', mine.player_id,
                        'player_count', (
                            select count(*)
                            from public.stellarion_game_players as all_players
                            where all_players.game_id = g.id
                        ),
                        'max_players', g.max_players
                    ) order by g.updated_at desc, g.id
                ),
                '[]'::jsonb
            )
    end
    from public.stellarion_games as g
    join public.stellarion_game_players as mine
      on mine.game_id = g.id and mine.user_id = auth.uid();
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

    update public.stellarion_games
       set state = p_persisted,
           max_players = v_player_count,
           status = 'active',
           persisted_schema_version = (p_persisted ->> 'schema_version')::integer,
           revision = revision + 1,
           updated_at = clock_timestamp()
     where id = p_game_id;
    perform public.stellarion_emit_event(p_game_id, 'game_started', null, null);
    return public.stellarion_game_record(p_game_id);
end;
$$;

create function public.stellarion_save_game(
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
    perform public.stellarion_validate_persisted(
        p_persisted, v_game.max_players, v_game.status, v_game.current_turn
    );

    update public.stellarion_games
       set state = p_persisted,
           persisted_schema_version = (p_persisted ->> 'schema_version')::integer,
           revision = revision + 1,
           updated_at = clock_timestamp()
     where id = p_game_id;
    perform public.stellarion_emit_event(p_game_id, 'state_changed', null, null);
    return public.stellarion_game_record(p_game_id);
end;
$$;

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
    v_existing text;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if p_submission is null or jsonb_typeof(p_submission) is distinct from 'object'
       or pg_column_size(p_submission) > 1048576
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
    exception
        when invalid_text_representation or numeric_value_out_of_range then
            raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:submission_ids';
    end;
    if v_player_id is null or v_player_id not between 1 and 4
       or v_turn is null or v_turn < 1 then
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
          and not coalesce((player ->> 'spectator')::boolean, false)
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;
    if v_turn is distinct from v_game.current_turn then
        raise exception using errcode = 'P0001',
            message = 'STLR_STALE_SUBMISSION:' || v_game.current_turn::text || ':' || v_turn::text;
    end if;

    v_digest := encode(
        digest(
            convert_to('stellarion-turn-submission-v1' || p_submission::text, 'UTF8'),
            'sha256'
        ),
        'hex'
    );
    select digest into v_existing
      from public.stellarion_turn_submissions
      where game_id = p_game_id and turn = v_turn and player_id = v_player_id;
    if found then
        if v_existing = v_digest then
            return jsonb_build_object('disposition', 'duplicate');
        end if;
        raise exception using errcode = 'P0001',
            message = 'STLR_DUPLICATE_SUBMISSION:' || v_player_id::text || ':' || v_turn::text;
    end if;

    insert into public.stellarion_turn_submissions (
        game_id, turn, player_id, submission, digest
    ) values (
        p_game_id, v_turn, v_player_id, p_submission, v_digest
    );
    perform public.stellarion_emit_event(
        p_game_id, 'turn_submitted', v_turn, v_player_id
    );
    return jsonb_build_object('disposition', 'inserted');
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
                       'digest', digest
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

    if exists (
        select 1
        from jsonb_array_elements(v_game.state #> '{state,players}') as players(player)
        where not coalesce((player ->> 'spectator')::boolean, false)
          and not exists (
              select 1
              from public.stellarion_turn_submissions as submission
              where submission.game_id = p_game_id
                and submission.turn = p_resolved_turn
                and submission.player_id = (player ->> 'id')::bigint
          )
    ) then
        raise exception using errcode = 'P0001', message = 'STLR_TURN_INCOMPLETE';
    end if;

    update public.stellarion_games
       set state = p_persisted,
           status = v_next_status,
           persisted_schema_version = (p_persisted ->> 'schema_version')::integer,
           current_turn = p_resolved_turn + 1,
           revision = revision + 1,
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
    if p_after_sequence is null or p_after_sequence < 0 then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:event_cursor';
    end if;

    with replay as (
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
           coalesce(max(replay.sequence), p_after_sequence)
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
           where game_id = p_game_id and not connected
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
    v_player public.stellarion_game_players%rowtype;
begin
    if auth.uid() is null then
        raise exception using errcode = 'P0001', message = 'STLR_UNAUTHENTICATED';
    end if;
    if p_connected is null then
        raise exception using errcode = 'P0001', message = 'STLR_INVALID_DATA:connected';
    end if;
    select * into v_player
      from public.stellarion_game_players
      where game_id = p_game_id and user_id = auth.uid()
      for update;
    if not found then
        if not exists (select 1 from public.stellarion_games where id = p_game_id) then
            raise exception using errcode = 'P0001', message = 'STLR_GAME_NOT_FOUND';
        end if;
        raise exception using errcode = 'P0001', message = 'STLR_FORBIDDEN';
    end if;

    update public.stellarion_game_players
       set connected = p_connected,
           last_seen_at = clock_timestamp()
     where game_id = p_game_id and player_id = v_player.player_id;

    if v_player.connected is distinct from p_connected then
        perform public.stellarion_emit_event(
            p_game_id,
            case when p_connected then 'player_connected' else 'player_disconnected' end,
            null,
            v_player.player_id
        );
    end if;
    return jsonb_build_object('ok', true);
end;
$$;

-- Tables are RPC-only except for the Realtime event stream. In particular,
-- recovery hashes are never selectable through the public client roles.
revoke all on table public.stellarion_games from anon, authenticated;
revoke all on table public.stellarion_game_players from anon, authenticated;
revoke all on table public.stellarion_turn_submissions from anon, authenticated;
revoke all on table public.stellarion_game_events from anon, authenticated;
grant select on table public.stellarion_game_events to authenticated;

revoke all on function public.stellarion_is_game_member(uuid) from public, anon;
grant execute on function public.stellarion_is_game_member(uuid) to authenticated;

revoke all on function public.stellarion_validate_persisted(jsonb, smallint, text, bigint)
    from public, anon, authenticated;
revoke all on function public.stellarion_game_record(uuid)
    from public, anon, authenticated;
revoke all on function public.stellarion_membership_record(uuid, uuid)
    from public, anon, authenticated;
revoke all on function public.stellarion_emit_event(uuid, text, bigint, bigint)
    from public, anon, authenticated;

revoke all on function public.stellarion_create_game(text, text, text, smallint, jsonb)
    from public, anon;
revoke all on function public.stellarion_join_game(text, text, text)
    from public, anon;
revoke all on function public.stellarion_recover_player(text, text, text)
    from public, anon;
revoke all on function public.stellarion_list_games()
    from public, anon;
revoke all on function public.stellarion_load_game(uuid)
    from public, anon;
revoke all on function public.stellarion_start_game(uuid, bigint, jsonb)
    from public, anon;
revoke all on function public.stellarion_resume_game(uuid)
    from public, anon;
revoke all on function public.stellarion_save_game(uuid, bigint, jsonb)
    from public, anon;
revoke all on function public.stellarion_submit_turn(uuid, jsonb)
    from public, anon;
revoke all on function public.stellarion_load_turn_submissions(uuid, bigint)
    from public, anon;
revoke all on function public.stellarion_publish_resolution(uuid, bigint, bigint, jsonb)
    from public, anon;
revoke all on function public.stellarion_events_since(uuid, bigint)
    from public, anon;
revoke all on function public.stellarion_set_connected(uuid, boolean)
    from public, anon;

grant execute on function public.stellarion_create_game(text, text, text, smallint, jsonb)
    to authenticated;
grant execute on function public.stellarion_join_game(text, text, text)
    to authenticated;
grant execute on function public.stellarion_recover_player(text, text, text)
    to authenticated;
grant execute on function public.stellarion_list_games()
    to authenticated;
grant execute on function public.stellarion_load_game(uuid)
    to authenticated;
grant execute on function public.stellarion_start_game(uuid, bigint, jsonb)
    to authenticated;
grant execute on function public.stellarion_resume_game(uuid)
    to authenticated;
grant execute on function public.stellarion_save_game(uuid, bigint, jsonb)
    to authenticated;
grant execute on function public.stellarion_submit_turn(uuid, jsonb)
    to authenticated;
grant execute on function public.stellarion_load_turn_submissions(uuid, bigint)
    to authenticated;
grant execute on function public.stellarion_publish_resolution(uuid, bigint, bigint, jsonb)
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

-- Ensure the Data API sees every RPC immediately after this fresh-project install.
notify pgrst, 'reload schema';

commit;
