#!/bin/sh
# Release smoke test (ADR 0098): runs deploy/compose.yaml against images already built or pulled,
# in its own compose project with no host ports, then removes it with `down -v`.
#   sh scripts/release/smoke.sh <image-prefix> <version> [--with-ai]
#   e.g. sh scripts/release/smoke.sh rematch-local 0.1.0
# It checks:
#   1. web answers /api/health with {"ok":true} against Postgres and Redis;
#   2. `worker --version` prints "worker <version>";
#   3. the worker starts, applies every db/migrations file, and keeps running;
#   4. each image carries /usr/share/doc/rematch/NOTICE (and the worker Stockfish's Copying.txt);
#   5. with --with-ai: ai-sidecar-role grants the role LOGIN and the sidecar turns healthy.
# SMOKE_PROJECT names the compose project (default ci-release).
set -eu

prefix=${1:?usage: smoke.sh <image-prefix> <version> [--with-ai]}
version=${2:?usage: smoke.sh <image-prefix> <version> [--with-ai]}
with_ai=${3:-}
root=$(cd "$(dirname "$0")/../.." && pwd)
# The path Docker gets: E:/... under Git Bash, the plain path elsewhere.
host_root=$(cd "$root" && { pwd -W 2>/dev/null || pwd; })
project=${SMOKE_PROJECT:-ci-release}

rand() { od -An -N24 -tx1 /dev/urandom | tr -d ' \n'; }
export REMATCH_IMAGE_PREFIX="$prefix" REMATCH_VERSION="$version"
export POSTGRES_PASSWORD="$(rand)" BETTER_AUTH_SECRET="$(rand)$(rand)"
export BETTER_AUTH_URL=http://localhost:3100 SMTP_URL=smtp://127.0.0.1:1025
export AI_SIDECAR_TOKEN="$(rand)" AI_SIDECAR_DB_PASSWORD="$(rand)"
# The worker's data mount: the git-ignored data/ of the checkout (an empty one is fine).
mkdir -p "$root/data"
export REMATCH_DATA_DIR="$host_root/data"
# The sidecar refuses to start in production without a real ANTHROPIC_API_KEY; the smoke test
# checks the image, not the key, so it runs the sidecar in development mode with no credential.
export AI_SIDECAR_ENV=development
# Git Bash would rewrite the in-container paths below (/usr/share/...) into Windows paths.
export MSYS_NO_PATHCONV=1
# Ignore any COMPOSE_* settings of the calling shell.
export COMPOSE_PROFILES= COMPOSE_FILE=

dc() {
  docker compose -p "$project" --project-directory "$host_root/deploy" \
    -f "$host_root/deploy/compose.yaml" -f "$host_root/scripts/release/smoke.compose.yaml" "$@"
}
fail() {
  echo "FAIL $1" >&2
  dc logs --no-color --tail 80 >&2 || true
  exit 1
}
cleanup() { dc --profile ai down -v --remove-orphans >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM

echo "== $project: web, Postgres, Redis ($prefix-*:$version)"
dc up -d --wait --wait-timeout 180 postgres redis web || fail "web did not turn healthy"
health=$(dc exec -T web node -e "fetch('http://127.0.0.1:3100/api/health').then(async (r) => { console.log(r.status, await r.text()); })")
echo "/api/health: $health"
case "$health" in
  '200 {"ok":true,"db":true,"redis":true}') ;;
  *) fail "/api/health answered: $health" ;;
esac

echo "== worker --version"
got=$(dc run --rm --no-deps -T worker --version)
[ "$got" = "worker $version" ] || fail "worker --version printed '$got', expected 'worker $version'"
echo "ok   $got"

echo "== licences in the images"
dc exec -T web test -s /usr/share/doc/rematch/NOTICE || fail "web: no /usr/share/doc/rematch/NOTICE"
dc exec -T web test -s /usr/share/doc/rematch/LICENSE || fail "web: no /usr/share/doc/rematch/LICENSE"
dc run --rm --no-deps -T --entrypoint sh worker -c \
  'test -s /usr/share/doc/rematch/NOTICE && test -s /usr/share/doc/rematch/LICENSE && test -s /usr/share/doc/stockfish/Copying.txt' \
  || fail "worker: a licence file is missing"
echo "ok   NOTICE, LICENSE (and Stockfish's Copying.txt in the worker)"

echo "== the worker starts and migrates the database"
want=$(ls "$root/db/migrations" | grep -c '\.sql$')
dc up -d worker
i=0
while :; do
  n=$(dc exec -T postgres psql -U postgres -d rematch -tAc \
    "select count(*) from _sqlx_migrations where success" 2>/dev/null | tr -d '[:space:]' || true)
  [ "${n:-0}" = "$want" ] && break
  i=$((i + 1))
  [ "$i" -ge 90 ] && fail "the worker applied ${n:-0} of $want migrations in 180 s"
  sleep 2
done
sleep 5
[ "$(dc ps --status running -q worker)" != "" ] || fail "the worker exited after migrating"
echo "ok   $want of $want migrations applied; the worker is running"

if [ "$with_ai" = "--with-ai" ]; then
  echo "== ai-sidecar"
  dc --profile ai run --rm -T ai-sidecar-role || fail "ai-sidecar-role failed"
  dc --profile ai up -d --wait --wait-timeout 120 ai-sidecar || fail "ai-sidecar did not turn healthy"
  dc --profile ai exec -T ai-sidecar test -s /usr/share/doc/rematch/NOTICE \
    || fail "ai-sidecar: no /usr/share/doc/rematch/NOTICE"
  echo "ok   ai-sidecar healthy, NOTICE present"
fi

echo "smoke: all checks passed for $prefix-*:$version"
