#!/bin/sh
# Regression test for ADR 0044: an orphaned process in the postgres container must not crash
# Postgres.
#
# Without an init process, the postmaster is PID 1 of the container. It is then the reaper for
# every orphan in that PID namespace, for example a `pg_isready` whose healthcheck shell Docker
# killed on timeout. The postmaster treats any child that exits with a code other than 0 or 1 as a
# crashed backend, so it terminates every backend and runs crash recovery. Tests then see EOF,
# "connection reset" and "the database system is in recovery mode".
#
# The test orphans a subshell that exits with 2 and asserts that Postgres did not reinitialise.
# It also asserts that the healthcheck runs without a shell wrapper and probes TCP, so it cannot
# pass against the unix-socket-only server that the image's init phase runs.
#
#   COMPOSE_FILE=docker-compose.yml sh scripts/test-postgres-orphan-reap.sh [project]
#
# It uses its own compose project (default ci-pg-orphan-test) and removes it with `down -v`.
set -eu

project="${1:-ci-pg-orphan-test}"
compose() { docker compose -p "$project" "$@"; }
cleanup() { compose down -v > /dev/null 2>&1 || true; }
trap cleanup EXIT

fail() {
  echo "test-postgres-orphan-reap: FAIL: $*" >&2
  exit 1
}

compose up -d --wait postgres > /dev/null 2>&1 || fail "postgres did not become healthy"
container="$(compose ps -q postgres)"

# Wait for the final server, which listens on TCP. The init phase's temporary server is not PID 1,
# so an orphan exiting during init would prove nothing.
tries=0
until docker exec "$container" pg_isready -q -h 127.0.0.1 -U postgres -d chess; do
  tries=$((tries + 1))
  [ "$tries" -lt 60 ] || fail "postgres never accepted TCP connections"
  sleep 1
done

# Orphan a process that exits with 2. `sh -c` returns at once, so the subshell is reparented to the
# container's PID 1.
docker exec "$container" sh -c '(sleep 1; exit 2) > /dev/null 2>&1 & exit 0'
sleep 4

if docker logs "$container" 2>&1 | grep -E 'exited with exit code 2|reinitializing'; then
  fail "an orphan exiting with code 2 crash-restarted Postgres (is init: true set?)"
fi

test_cmd="$(docker inspect -f '{{json .Config.Healthcheck.Test}}' "$container")"
case "$test_cmd" in
  '["CMD-SHELL"'*) fail "healthcheck runs through a shell: $test_cmd" ;;
esac
case "$test_cmd" in
  *'"-h","127.0.0.1"'*) ;;
  *) fail "healthcheck must probe TCP (-h 127.0.0.1): $test_cmd" ;;
esac

docker exec "$container" psql -U postgres -d chess_test -Atc 'select 1' > /dev/null \
  || fail "postgres is not answering after the orphan exited"

echo "test-postgres-orphan-reap: ok"
