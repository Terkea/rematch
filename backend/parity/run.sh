#!/bin/sh
# Parity acceptance (R1 Task 24): the TS worker and the Rust worker analyse the same fixture games,
# each into its own database, and `parity compare` checks the results row by row.
#
#   sh backend/parity/run.sh            (from the repo root, or anywhere inside it)
#
# It runs in its own compose project (default ci-parity, no host ports; override with
# PARITY_PROJECT) and never touches the default `chess-improver` project. Logs go to
# PARITY_LOGS (default: a fresh temp dir). Exit code: the one of `parity compare` (0 on parity,
# 1 on mismatches), 2 on a timeout or a setup failure.
set -eu

cd "$(dirname "$0")/../.."
export COMPOSE_FILE=docker-compose.yml
export MSYS_NO_PATHCONV=1
P="${PARITY_PROJECT:-ci-parity}"
case "$P" in
  chess-improver) echo "parity: refusing to run in the default project" >&2; exit 2 ;;
esac
LOGS="${PARITY_LOGS:-$(mktemp -d)}"
mkdir -p "$LOGS"
TIMEOUT_S="${PARITY_TIMEOUT_S:-1200}"
PG=postgres://postgres:postgres@postgres:5432
TS_DB=chess_parity_ts
RS_DB=chess_parity_rs
TS_C="$P-parity-ts-worker"
RS_C="$P-parity-rs-worker"

dc() { docker compose -p "$P" "$@"; }
psql_c() { dc exec -T postgres psql -U postgres -d chess -tAq -v ON_ERROR_STOP=1 -c "$1"; }
parity() { dc run --rm -T -e LANE=parity rust-dev cargo run --locked -q -p parity -- "$@"; }

stop_workers() {
  # SIGTERM; both drain within their 25 s grace.
  docker stop -t 30 "$TS_C" "$RS_C" >/dev/null 2>&1 || true
}
trap stop_workers EXIT INT TERM

echo "parity: project $P, logs in $LOGS"
dc up -d --wait postgres redis
dc build -q rust-worker worker

for db in "$TS_DB" "$RS_DB"; do
  psql_c "DROP DATABASE IF EXISTS $db WITH (FORCE)"
  psql_c "CREATE DATABASE $db"
  dc run --rm -T --no-deps --entrypoint cli rust-worker db migrate --database-url "$PG/$db"
done
# Each TS worker's Redis is this project's own: start it empty.
dc exec -T redis redis-cli FLUSHALL >/dev/null

# The fixture set is analysed in phases (`parity phases`): each Lichess-eval game alone, then the
# rest. The eval cache keeps the first write per position, so this makes the cache state each
# game reads the same on both workers (see `seed_phase` in tools/parity/src/seed.rs).
PHASES=$(parity phases | tail -n 1)
seed_phase() {
  parity seed --database-url "$PG/$TS_DB" --phase "$1"
  parity seed --database-url "$PG/$RS_DB" --phase "$1" --enqueue
}
ts_enqueue="pnpm --filter @ci/worker exec tsx --conditions=development scripts/enqueue-pending.ts"

open_games() {
  dc exec -T postgres psql -U postgres -d "$1" -tAq -c \
    "SELECT count(*) FROM games WHERE analysis_status IN ('pending', 'analysing')"
}
start=$(date +%s)
wait_done() {
  while :; do
    ts=$(open_games "$TS_DB" || echo "?")
    rs=$(open_games "$RS_DB" || echo "?")
    if [ "$ts" = 0 ] && [ "$rs" = 0 ]; then break; fi
    if [ $(($(date +%s) - start)) -ge "$TIMEOUT_S" ]; then
      echo "parity: timed out after ${TIMEOUT_S}s (open games: ts $ts, rs $rs)" >&2
      exit 2
    fi
    sleep 3
  done
  echo "parity: phase $1 of $PHASES done after $(($(date +%s) - start))s"
}

seed_phase 0
# The TS worker reaches no platform here (no import jobs). CONTACT_EMAIL only has to be non-empty
# for its startup check, and is not an address (ADR 0043).
dc run --rm -T --name "$TS_C" -e DATABASE_URL="$PG/$TS_DB" -e CONTACT_EMAIL=parity-no-network \
  worker sh -c "$ts_enqueue && exec pnpm --filter @ci/worker exec tsx --conditions=development src/main.ts" \
  >"$LOGS/ts-worker.log" 2>&1 &
dc run --rm -T --name "$RS_C" -e DATABASE_URL="$PG/$RS_DB" rust-worker \
  >"$LOGS/rs-worker.log" 2>&1 &
wait_done 1

k=1
while [ "$k" -lt "$PHASES" ]; do
  seed_phase "$k"
  dc run --rm -T --no-deps -e DATABASE_URL="$PG/$TS_DB" worker sh -c "$ts_enqueue"
  wait_done $((k + 1))
  k=$((k + 1))
done
echo "parity: both workers finished in $(($(date +%s) - start))s"
stop_workers
wait || true

set +e
parity compare --a "$PG/$TS_DB" --b "$PG/$RS_DB" | tee "$LOGS/compare.txt"
code=$?
set -e
# POSIX sh has no pipefail: read the compare exit code from its last line instead.
if tail -n 1 "$LOGS/compare.txt" | grep -q ', 0 mismatches$'; then code=0; else code=1; fi
exit "$code"
