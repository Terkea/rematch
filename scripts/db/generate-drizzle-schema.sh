#!/bin/sh
# Regenerates packages/db/src/generated/ (the web app's Drizzle schema) from db/migrations/ (ADR 0033).
#   1. creates a scratch database `schema_pull` (dropped first if it exists),
#   2. applies every db/migrations/*.sql in version order (psql -1, ON_ERROR_STOP),
#   3. runs `drizzle-kit pull` against it (@ci/db db:pull),
#   4. formats the output with Biome and drops the database.
# Run from the repository root (needs Docker). Rerun it as the last step of any migration change and
# commit the result; the CI job `schema-drift` fails when the committed files differ.
# To use another compose project, set COMPOSE, e.g.
#   COMPOSE="docker compose -p ci-r1-t15" sh scripts/db/generate-drizzle-schema.sh
set -eu

COMPOSE=${COMPOSE:-docker compose}
DB=schema_pull
OUT=packages/db/src/generated

psql_in() { # psql_in <db> [psql args...]  (SQL on stdin)
  db=$1; shift
  $COMPOSE exec -T postgres psql -X -q -U postgres -d "$db" -v ON_ERROR_STOP=1 "$@"
}

cleanup() { psql_in postgres -c "DROP DATABASE IF EXISTS $DB" || true; }
trap cleanup EXIT

$COMPOSE up -d --wait postgres > /dev/null
psql_in postgres -c "DROP DATABASE IF EXISTS $DB" -c "CREATE DATABASE $DB"

# The glob expands in lexical order, which is version order (fixed-width timestamps).
for f in db/migrations/*.sql; do
  echo "migrate: $f" >&2
  psql_in "$DB" -1 < "$f"
done

# Start from an empty directory so a dropped table cannot linger in the output.
rm -rf "$OUT"
$COMPOSE run --rm -T -e DATABASE_URL="postgres://postgres:postgres@postgres:5432/$DB" \
  worker pnpm --filter @ci/db db:pull
# drizzle-kit pull also writes migration SQL and a meta/ folder; only the TypeScript is kept.
rm -rf "$OUT/meta" "$OUT"/*.sql
# ESM: relative imports carry the .js extension (repo convention, nodenext).
sed -i "s#from \"./schema\"#from \"./schema.js\"#" "$OUT/relations.ts"
$COMPOSE run --rm -T worker pnpm exec biome format --write "$OUT"
$COMPOSE run --rm -T worker pnpm exec biome check --write --unsafe "$OUT"

ls "$OUT" >&2
