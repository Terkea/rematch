#!/bin/sh
# Proves the sqlx baseline migration produces exactly the schema the three Drizzle migrations do
# (ADR 0033). Creates `schema_drizzle` (migrated by drizzle-kit) and `schema_sqlx` (migrated with
# only db/migrations/20261121000000_m1_baseline.sql), then diffs
#   1. `pg_dump --schema-only --no-owner --no-privileges` (Drizzle's own bookkeeping excluded), and
#   2. a catalogue introspection (columns, constraints, indexes, enum labels, sequences).
# Expected output: nothing, exit 0. The two databases are dropped on exit.
#
# Run from the repository root (needs Docker; the compose `postgres` service supplies psql and
# pg_dump of the server's own version, the `worker` service supplies drizzle-kit):
#   sh backend/scripts/check-baseline-schema.sh
# To use another compose project or extra -f files, set COMPOSE, e.g.
#   COMPOSE="docker compose -p ci-r1-t12 -f docker-compose.yml -f no-ports.yml" sh backend/scripts/...
set -eu

COMPOSE=${COMPOSE:-docker compose}
BASELINE=${BASELINE:-db/migrations/20261121000000_m1_baseline.sql}
PGUSER=postgres
WORK=$(mktemp -d)

psql_in() { # psql_in <db> [psql args...]  (SQL on stdin)
  db=$1; shift
  $COMPOSE exec -T postgres psql -X -q -U "$PGUSER" -d "$db" -v ON_ERROR_STOP=1 "$@"
}

cleanup() {
  psql_in postgres -c 'DROP DATABASE IF EXISTS schema_drizzle' || true
  psql_in postgres -c 'DROP DATABASE IF EXISTS schema_sqlx' || true
  rm -rf "$WORK"
}
trap cleanup EXIT

$COMPOSE up -d --wait postgres > /dev/null

psql_in postgres -c 'DROP DATABASE IF EXISTS schema_drizzle' -c 'DROP DATABASE IF EXISTS schema_sqlx' \
  -c 'CREATE DATABASE schema_drizzle' -c 'CREATE DATABASE schema_sqlx'

# 1. Drizzle: the three historical migrations.
$COMPOSE run --rm -T -e DATABASE_URL="postgres://$PGUSER:postgres@postgres:5432/schema_drizzle" \
  worker pnpm --filter @ci/db exec drizzle-kit migrate

# Drizzle's recorded timestamps must equal db::baseline::DRIZZLE_MIGRATION_TIMESTAMPS (adoption
# checks them).
ts=$(psql_in schema_drizzle -A -t -c 'SELECT string_agg(created_at::text, $$,$$ ORDER BY id) FROM drizzle.__drizzle_migrations')
want=$(grep 'DRIZZLE_MIGRATION_TIMESTAMPS: ' backend/crates/db/src/baseline.rs | grep -o '[0-9]\{13\}' | paste -sd, -)
[ "$ts" = "$want" ] || { echo "drizzle created_at $ts != baseline.rs $want" >&2; exit 1; }

# 2. sqlx: only the baseline file, in one transaction.
psql_in schema_sqlx -1 < "$BASELINE"

dump() {
  $COMPOSE exec -T postgres pg_dump -U "$PGUSER" --schema-only --no-owner --no-privileges \
    --exclude-schema=drizzle "$1" | grep -Ev '^[\\](un)?restrict '
}

CATALOGUE="
SELECT 'column', table_name, ordinal_position, column_name, data_type, udt_name,
       is_nullable, coalesce(column_default, ''), coalesce(character_maximum_length::text, '')
  FROM information_schema.columns WHERE table_schema = 'public'
UNION ALL
SELECT 'constraint', c.conrelid::regclass::text, 0, c.conname, c.contype::text,
       pg_get_constraintdef(c.oid), '', '', ''
  FROM pg_constraint c JOIN pg_namespace n ON n.oid = c.connamespace WHERE n.nspname = 'public'
UNION ALL
SELECT 'index', tablename, 0, indexname, indexdef, '', '', '', ''
  FROM pg_indexes WHERE schemaname = 'public'
UNION ALL
SELECT 'enum', t.typname, e.enumsortorder::int, e.enumlabel, '', '', '', '', ''
  FROM pg_enum e JOIN pg_type t ON t.oid = e.enumtypid
  JOIN pg_namespace n ON n.oid = t.typnamespace WHERE n.nspname = 'public'
UNION ALL
SELECT 'sequence', sequencename, 0, data_type::text, start_value::text, increment_by::text,
       min_value::text, max_value::text, cycle::text
  FROM pg_sequences WHERE schemaname = 'public'
ORDER BY 1, 2, 3, 4"

dump schema_drizzle > "$WORK/drizzle.dump.sql"
dump schema_sqlx > "$WORK/sqlx.dump.sql"
echo "$CATALOGUE" | psql_in schema_drizzle -A -t > "$WORK/drizzle.catalogue.txt"
echo "$CATALOGUE" | psql_in schema_sqlx -A -t > "$WORK/sqlx.catalogue.txt"

[ -s "$WORK/drizzle.dump.sql" ] || { echo "empty pg_dump of schema_drizzle" >&2; exit 2; }
[ -s "$WORK/drizzle.catalogue.txt" ] || { echo "empty catalogue of schema_drizzle" >&2; exit 2; }

status=0
diff -u "$WORK/drizzle.dump.sql" "$WORK/sqlx.dump.sql" || status=1
diff -u "$WORK/drizzle.catalogue.txt" "$WORK/sqlx.catalogue.txt" || status=1
exit $status
