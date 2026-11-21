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

### Resolving workspace source in `next dev`

Next 16 exposes no custom export condition, and Turbopack has no `extensionAlias`, so it cannot resolve the `./client.js` imports in `@ci/db` to `client.ts` (checked: "Module not found: Can't resolve './client.js'"). So `next dev` runs with `--webpack`, `resolve.extensionAlias` maps `.js` to `.ts`, and in development only `@ci/db` and `@ci/chess-core` are aliased to their `src/index.ts`. Production builds (`next build`, Turbopack) use `dist`, which turbo's `^build` produces first.

### Hot reload

Docker Desktop bind mounts on Windows deliver no file events to Linux, so `WATCHPACK_POLLING=true` is set on `web` from the start. A host edit of `page.tsx` reached `curl localhost:3100/` in about 4 s.

## Alternatives considered

Turbopack for dev (fails on the `.js` imports); stripping `.js` from `@ci/db` (breaks its NodeNext build); a separate Compose project for the web stack (splits the shared database).

## Consequences

`docker compose up -d` brings up web and Mailpit with Postgres and Redis. Every new workspace package needs a `node_modules` volume in the anchor.
