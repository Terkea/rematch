#!/bin/sh
# Regenerates packages/db/src/generated/ (the web app's Drizzle schema) from db/migrations/ (ADR 0033).
#   1. creates a scratch database `schema_pull_<pid>_<time>` (unique per run, so concurrent runs and
#      other databases never collide; the trap drops only this one),
#   2. applies every db/migrations/*.sql in version order (psql -1, ON_ERROR_STOP),
#   3. runs `drizzle-kit pull` into a scratch directory, normalises it (packages/db/scripts/
#      normalise-generated.mjs) and formats it with Biome,
#   4. only if everything succeeded, copies schema.ts and relations.ts into place.
# The committed output is never touched before the pull has succeeded.
# Run from the repository root (needs Docker). Rerun it as the last step of any migration change and
# commit the result; the CI job `schema-drift` fails when the committed files differ.
# To use another compose project, set COMPOSE, e.g.
#   COMPOSE="env COMPOSE_FILE=docker-compose.yml docker compose -p ci-r1-t15" sh scripts/db/generate-drizzle-schema.sh
set -eu

COMPOSE=${COMPOSE:-docker compose}
DB="schema_pull_$$_$(date +%s)"
OUT=packages/db/src/generated
TMP=".pull-tmp-$$"   # relative to packages/db, as seen from the worker container

psql_in() { # psql_in <db> [psql args...]  (SQL on stdin)
  db=$1; shift
  $COMPOSE exec -T postgres psql -X -q -U postgres -d "$db" -v ON_ERROR_STOP=1 "$@"
}

cleanup() {
  psql_in postgres -c "DROP DATABASE IF EXISTS $DB" || true
  # removed inside the container: the files there are owned by root
  $COMPOSE run --rm -T worker sh -c "rm -rf /app/packages/db/$TMP" || true
}
trap cleanup EXIT

$COMPOSE up -d --wait postgres > /dev/null
psql_in postgres -c "CREATE DATABASE $DB"

# The glob expands in lexical order, which is version order (fixed-width timestamps).
for f in db/migrations/*.sql; do
  echo "migrate: $f" >&2
  psql_in "$DB" -1 < "$f"
done

# One container run: pull, normalise (fails loudly when a rule no longer matches), format.
$COMPOSE run --rm -T \
  -e DATABASE_URL="postgres://postgres:postgres@postgres:5432/$DB" -e PULL_OUT="./$TMP" \
  worker sh -c "
    cd /app && pnpm --filter @ci/db db:pull &&
    test -f packages/db/$TMP/schema.ts && test -f packages/db/$TMP/relations.ts &&
    node packages/db/scripts/normalise-generated.mjs packages/db/$TMP &&
    pnpm exec biome check --write --unsafe packages/db/$TMP/schema.ts packages/db/$TMP/relations.ts
  "

# Success: move into place (overwrite the two files; nothing else lives there).
mkdir -p "$OUT"
cp "packages/db/$TMP/schema.ts" "packages/db/$TMP/relations.ts" "$OUT/"
ls "$OUT" >&2
