-- Notable: esquema remoto para la sincronización.
-- Ejecutar UNA vez en Supabase → SQL Editor → New query → pegar todo → Run.
-- Es genérico a propósito: cada nodo del árbol (carrera, ramo, nota, etc.) es una fila
-- y su contenido va en `data` (JSON). Los cambios del modelo en la app no requieren tocar esto.

create table if not exists public.items (
    id                uuid        primary key,
    user_id           uuid        not null default auth.uid()
                                  references auth.users (id) on delete cascade,
    parent_id         uuid,
    data              jsonb       not null default '{}'::jsonb,
    updated_at        timestamptz not null,               -- cuándo lo cambió el usuario (dispositivo)
    deleted           boolean     not null default false, -- borrado lógico, para sincronizarlo
    server_updated_at timestamptz not null default now()  -- cuándo llegó al servidor (para bajar cambios)
);

create index if not exists items_user_server_updated_idx
    on public.items (user_id, server_updated_at);

-- Gana el cambio más reciente: si llega una versión más vieja que la guardada, se ignora.
create or replace function public.items_before_write()
returns trigger
language plpgsql
set search_path = ''
as $$
begin
    if tg_op = 'UPDATE' then
        if new.updated_at < old.updated_at then
            return old;               -- versión vieja: se mantiene la del servidor
        end if;
        new.user_id := old.user_id;   -- nadie puede cambiar el dueño de una fila
    end if;
    new.server_updated_at := now();
    return new;
end;
$$;

drop trigger if exists items_before_write on public.items;
create trigger items_before_write
    before insert or update on public.items
    for each row execute function public.items_before_write();

-- Seguridad: cada usuario solo ve y modifica sus propias filas.
alter table public.items enable row level security;

-- Acceso por la API solo con sesión iniciada (el rol anónimo no puede tocar la tabla).
-- Hace falta si el proyecto tiene desactivado "Automatically expose new tables".
revoke all on table public.items from anon;
grant select, insert, update, delete on table public.items to authenticated;

drop policy if exists "items_select_own" on public.items;
create policy "items_select_own" on public.items
    for select to authenticated
    using ((select auth.uid()) = user_id);

drop policy if exists "items_insert_own" on public.items;
create policy "items_insert_own" on public.items
    for insert to authenticated
    with check ((select auth.uid()) = user_id);

drop policy if exists "items_update_own" on public.items;
create policy "items_update_own" on public.items
    for update to authenticated
    using ((select auth.uid()) = user_id)
    with check ((select auth.uid()) = user_id);

drop policy if exists "items_delete_own" on public.items;
create policy "items_delete_own" on public.items
    for delete to authenticated
    using ((select auth.uid()) = user_id);
