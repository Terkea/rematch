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
docker compose up -d --build               # Postgres 17, Redis 7, worker with Stockfish 19, web, Mailpit
docker compose run --rm worker pnpm test   # full suite inside the container
docker compose run --rm worker pnpm lint
```

Postgres and Redis are published on `127.0.0.1:55432` and `127.0.0.1:56379` (ADR 0023). The web app is on
`http://127.0.0.1:3100` (`/api/health` reports Postgres and Redis) and the Mailpit UI on `http://127.0.0.1:48025`.
`docker compose --profile e2e up -d fakes` adds the fake providers on `127.0.0.1:53199`.
`docker/postgres/init.sql` creates the `chess_test`, `chess_worker_test` and `chess_web_test` databases only on a fresh `pgdata` volume. On an
existing volume, create it once with:

```sh
docker compose exec postgres psql -U postgres -c "CREATE DATABASE chess_test"
```

The worker's tests use their own database, `chess_worker_test` (`WORKER_DATABASE_URL_TEST`), so
package test suites can run in parallel without truncating each other's rows. Likewise:

```sh
docker compose exec postgres psql -U postgres -c "CREATE DATABASE chess_worker_test"
```

The web app's tests use `chess_web_test` (`WEB_DATABASE_URL_TEST`):

```sh
docker compose exec postgres psql -U postgres -c "CREATE DATABASE chess_web_test"
```
