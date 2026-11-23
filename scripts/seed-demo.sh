#!/bin/sh
# `pnpm seed:demo [--email you@example.com] [--remove]` (ADR 0097): loads a fictional player's
# analysed games into the dev database of the running stack, so every page has content offline.
# It runs inside the compose `web` container (no host ports or host Node needed), and only ever
# touches the demo player's own rows. The email defaults to the first INVITE_ALLOWLIST entry.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
if [ -z "$(docker compose ps --status running -q web 2> /dev/null || true)" ]; then
  echo "seed:demo: the web container is not running. Start the stack first: sh scripts/setup.sh" >&2
  exit 1
fi
exec docker compose exec -T web sh -c \
  'cd apps/web && exec node_modules/.bin/tsx --conditions development scripts/seed-demo.ts "$@"' \
  seed-demo "$@"
