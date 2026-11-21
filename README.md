# chess-improver

A chess coaching platform: analysis of your games, improvement plans and training. TypeScript monorepo (pnpm + Turborepo).

## Getting started

```sh
nvm use            # Node 22
npm i -g pnpm@12   # or corepack, if your corepack supports pnpm 12
pnpm install
pnpm build && pnpm test && pnpm lint && pnpm typecheck
```

Layout: `apps/*` (web, worker), `packages/*` (shared libraries, scope `@ci/*`), `services/*`.
Design decisions live in `docs/decisions/`; the spec is in `docs/superpowers/specs/`.

## Local stack

```sh
cp .env.example .env                       # set CONTACT_EMAIL (never a real personal address)
docker compose up -d --build               # Postgres 17, Redis 7, worker with Stockfish 19
docker compose run --rm worker pnpm test   # full suite inside the container
docker compose run --rm worker pnpm lint
```

Postgres and Redis are published on `127.0.0.1:55432` and `127.0.0.1:56379` (ADR 0023).
`docker/postgres/init.sql` creates the `chess_test` and `chess_worker_test` databases only on a fresh `pgdata` volume. On an
existing volume, create it once with:

```sh
docker compose exec postgres psql -U postgres -c "CREATE DATABASE chess_test"
```

The worker's tests use their own database, `chess_worker_test` (`WORKER_DATABASE_URL_TEST`), so
package test suites can run in parallel without truncating each other's rows. Likewise:

```sh
docker compose exec postgres psql -U postgres -c "CREATE DATABASE chess_worker_test"
```

## Analyse CLI

Imports an account's games and analyses them in-process, then prints a summary and exits
(0 on success; 1 if the user is not found or every game failed):

```sh
docker compose run --rm worker pnpm --filter @ci/worker cli analyse \
  --platform <lichess|chesscom> --username <name> [--max N] [--timeout MINUTES]
# games: 20  analysed: 20  skipped: 0  failed: 0  avg accuracy: 65.1  blunders/game: 3.20
```

Chess.com requires a real `CONTACT_EMAIL` in `.env` for its User-Agent; `--timeout` defaults to 30 minutes.

Note: the CLI writes into the main `chess` database (`DATABASE_URL`), not a test database, and it does not
schedule the reconciler. SIGINT/SIGTERM stop the wait, shut the workers down cleanly and exit 130/143;
a second signal exits immediately.
