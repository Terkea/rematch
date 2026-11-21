# ADR-WEB-2. Web development stack in Compose

Date: 2026-11-21
Status: Accepted

## Context

The web app, a mail catcher, fake OAuth providers and a visual-QA runner must run in the same Compose project as Postgres, Redis and the worker, without disturbing them (ADR 0023).

## Decision

New services in `docker-compose.yml`:

- `deps`: one-shot `pnpm install --frozen-lockfile` into the shared volumes. `worker`, `web`, `fakes` and `qa` depend on it (`service_completed_successfully`) and set `SKIP_INSTALL=1`, which `apps/worker/docker-entrypoint.sh` honours, so two installs never race.
- `web`: `apps/web/Dockerfile` target `dev`, `next dev --webpack --port 3100 --hostname 0.0.0.0`, published on `127.0.0.1:3100`, with a health check on `/api/health`.
- `mailpit`: `axllent/mailpit:v1.34.0`. UI on `127.0.0.1:48025`. SMTP 1025 is internal only.
- `fakes` (profile `e2e`): fake Lichess and chess.com on `127.0.0.1:53199` (container 3199).
- `qa` (profile `qa`): `mcr.microsoft.com/playwright:v1.65.1-noble` for visual baselines.

`docker-compose.e2e.yml` points `web` at the fakes. New volumes: `web_node_modules`, `ui_node_modules` and `web_next` (keeps `.next` off the host bind mount). All workspace volumes are shared through one YAML anchor.

A third test database, `chess_web_test` (`WEB_DATABASE_URL_TEST`), is created by `docker/postgres/init.sql` on fresh volumes. On an existing volume run `docker compose exec postgres psql -U postgres -c "CREATE DATABASE chess_web_test"`.

### Ports

The plan reserved Mailpit's UI on 58025. On Windows that port sits inside a TCP excluded range (58019-58118, Hyper-V/WinNAT, listed by `netsh int ipv4 show excludedportrange protocol=tcp`), and Docker fails to bind it ("forbidden by its access permissions"). The ranges move between reboots and cover most of 49152 and above, so the port is moved below the dynamic range: Mailpit UI is `127.0.0.1:48025`. Web `3100` is unaffected. The fakes port `53199` is inside the dynamic range and is free today; if it becomes excluded, choose another port below 49152.

### Resolving workspace source: webpack for dev and build

Next 16 exposes no custom export condition, and Turbopack has no `extensionAlias`, so it cannot resolve the `./client.js` imports in `@ci/db` to `client.ts` (checked: "Module not found: Can't resolve './client.js'"). Next 16 also builds with Turbopack by default and rejects a `webpack()` config without a `turbopack` key. So one approach is used everywhere: **webpack** (`next dev --webpack`, `next build --webpack`). `resolve.extensionAlias` maps `.js` to `.ts`, and in development only `@ci/db` and `@ci/chess-core` are aliased to their `src/index.ts`. In production builds (`NODE_ENV=production`) there is no alias, so both resolve through their normal export map to `dist`. `@ci/web#build` inherits turbo's `^build`, which builds `@ci/db` and `@ci/chess-core` first. `pnpm turbo run build --filter @ci/web...` was run in Docker and succeeds.

### Production image

`apps/web/Dockerfile` has a `build` stage (install, `turbo run build --filter @ci/web...`) and a `runtime` stage on Next `output: "standalone"` (`outputFileTracingRoot` is the repo root). It runs as the non-root `node` user, contains no dev dependencies, no source mount and no secrets: all configuration is passed as environment variables at run time, and `.dockerignore` excludes `.env*`. Build with `docker build --target runtime -t chess-improver-web -f apps/web/Dockerfile .`.

### Dev-only Compose defaults

`docker-compose.yml` is a local development file and is never used for deployment. It sets `LAB_ENABLED=1` (the `/lab` pages are on) and a dev-only `BETTER_AUTH_SECRET` default. `env.ts` itself has no defaults for secrets and requires `BETTER_AUTH_SECRET` to be at least 32 characters. Flags (`CHESSCOM_ENABLED`, `CHESSCOM_OAUTH_ENABLED`, `LAB_ENABLED`) are on only for the exact value `1`.

### Hot reload

Docker Desktop bind mounts on Windows deliver no file events to Linux, so `WATCHPACK_POLLING=true` is set on `web` from the start. A host edit of `page.tsx` reached `curl localhost:3100/` in about 4 s.

## Alternatives considered

Turbopack (fails on the `.js` imports) or mixed Turbopack build with webpack dev (build errors on the webpack config); stripping `.js` from `@ci/db` (breaks its NodeNext build); a separate Compose project for the web stack (splits the shared database).

## Consequences

`docker compose up -d` brings up web and Mailpit with Postgres and Redis. Every new workspace package needs a `node_modules` volume in the anchor.
