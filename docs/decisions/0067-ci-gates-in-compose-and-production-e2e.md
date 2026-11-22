# 0067. CI runs the gates in compose, and e2e on a production build

Date: 2026-11-22. Status: accepted (M4 Task 27). Builds on 0022 (Biome), 0042 (Rust caches), 0065 (the throttle budget waits for a production e2e lane), ADR-WEB-2 (web dev stack) and ADR-WEB-8 (live events).

## Context

The CI workflow had drifted from how the gates run locally. The TS job ran on the runner with Postgres and Redis service containers, but with no `BETTER_AUTH_SECRET`, no `chess_web_test` database and no Stockfish. So the web tests could not start, and the worker and db tests that need an engine skipped. Nothing checked Biome formatting, so `pnpm format` on a clean checkout rewrote files (T26 hand-off). Nothing ran the design ban lint. The e2e suite ran only by hand, so the 4× CPU throttle budget of ADR 0065 was measured by hand too. The Rust job never fetched the Syzygy tables, though under `IN_DOCKER=1` the real-table tests fail without them.

## Decision

- **Every CI job runs its gate the way it runs locally:** in the compose services, in its own project with no host ports, then `down -v`. The TS gate runs every package but web in the `worker` container. That container has Stockfish, `DATABASE_URL_TEST`, `WORKER_DATABASE_URL_TEST` and `IN_DOCKER=1`, so a missing database or engine fails instead of skipping. Web runs in the `web` container, with compose's dev-only `BETTER_AUTH_SECRET` default and `WEB_DATABASE_URL_TEST`. The compose Postgres creates every test database on init. Runner-side service containers were dropped: a second way to provide the same environment would drift again.
- **No secret in any workflow.** The TS jobs use compose's public dev default, which signs only test sessions. A production server refuses that default (`env.ts`, `DEV_AUTH_SECRET`, or anything starting `dev-only-`), so the production e2e lane makes its own random, masked secret per run (`openssl rand -hex 32` into `E2E_BETTER_AUTH_SECRET`); `docker-compose.e2e-prod.yml` stops when it is unset. That guard is what keeps a compose-based deploy that forgets the variable from booting with a published signing key (review-t27 I2).
- **Least privilege.** The workflow grants `permissions: contents: read` at the top; no job needs more (cache and upload-artifact use the runner's token). A `concurrency` group per ref cancels superseded runs.
- **Format drift fails CI.** The new root `pnpm format:check` (`biome format .`) runs before the build. The drift was committed once, as `style: biome format`. Biome no longer formats Rust-owned generated JSON (`backend/fixtures/**`, `backend/crates/*/schema/**`): `serde_json` and the generators write those files, so formatting them would only make them drift again.
- **`pnpm design:check` runs in CI.**
- **e2e on a production build.** `docker-compose.e2e-prod.yml` layers on `docker-compose.e2e.yml`: web runs `turbo run build --filter=@ci/web...`, then `next start`, with `LAB_ENABLED=1` for the styleguide. The `qa` run sets `E2E_PRODUCTION=1`. So the trend's keyboard budget (key to paint within 16 ms at a 4× throttle, ADR 0065) is now enforced on every change. `next start` warns that `output: standalone` prefers `server.js`. The warning is accepted: the standalone server is the deploy image's job (the Dockerfile `runtime` target), and `next start` serves the same build.
- **The http fakes stay on the dev server.** A production server refuses plain-http Lichess and chess.com base URLs, by design. So the prod lane points them at the real https hosts, which nothing calls, and skips the specs tagged `@fakes`: the chess.com location-code flow and the Lichess OAuth flow. Those two run in their own job, `e2e-fakes`, against `next dev`. Visual baselines stay a local check. They are made in the `qa` image on the dev server, and comparing them in CI needs a ruling on dev and production pixel parity first (T28).
- **The Rust job fetches the Syzygy 3-4-5 tables**, through `lichess-db syzygy`, which checks every file's SHA-256 against the pinned manifest (ADR-R2-22). They are cached by the manifest's hash, so a fetch happens only when the manifest changes.
- **Actions stay on their major tags** (`@v4`), as the repo already pinned them. Pinning by full SHA is a separate, repo-wide change.

## Integration fixes found by the e2e (T27)

- **The development alias broke `/api/events`.** `next.config.ts` aliased `@ci/db` to `src/index.ts`, and webpack applies an alias as a prefix. So `@ci/db/generated` resolved to `src/index.ts/generated`, and the events route failed to compile under `next dev`. The aliases are now exact (`@ci/db$`, `@ci/chess-core$`), and the `./generated` and `./generated/relations` subpaths map to their source files.
- **"Read the cursor before the page data" needs every page.** The App Router renders a layout and its page in parallel, so a cursor read only in the layout can come after the page's queries. `requestEventCursor` wraps `eventCursor` in React `cache()`, so it is read once per request. Every signed-in page awaits it before its data, and the layout passes the same value to `RealtimeProvider`.

## Logging (review-t27 I1)

Better Auth's logger, the background-task logs (magic-link delivery) and the Lichess link hook all go through `redactedLog`. It redacts every email address in the message and, recursively, in its arguments: an Error's message, stack, `cause` and own fields (a pg `code` and `detail`, nodemailer's `response`, `rejected` and `envelope`), which are kept for diagnosis. A Drizzle query error carries the bound address in its message, so this is the realistic failure path.

## What the production lane changed in the specs

- **Sign-in rate limit per client.** With no trusted proxy, every request lands in one shared bucket per path (3 magic links per 10 minutes, ADR-WEB-3): Better Auth keys it as 127.0.0.1 under `next dev`, and as "no IP" in production. The faster production suite ran through it, and with `realtime.spec.ts` the dev suite did too. `docker-compose.e2e.yml` now trusts localhost as the proxy, which is where the `qa` browser runs, and `signInByMagicLink` sends each user's own `x-forwarded-for` from the documentation range 203.0.113.0/24. This is the production setup ADR-WEB-3 already requires for friend access.
- **Streamed pages.** A production build streams `loading.tsx` boundaries: the content first lands in a hidden `<div id="S:n">`, and React swaps it in a moment later. Strict locators saw both copies. `gotoSettled` now waits for the document to finish parsing, and for those nodes to go, for up to 5 s, because a `notFound()` page keeps one.
- **The throttle budget fails today, on this base too.** On the production build the key costs one extra frame at the median: about 16.2 to 16.9 ms over five runs, against a budget of 16. Task 13 measured about 0.2 ms. The base commit before Task 27 gives the same 16.2 and 16.4 ms, so this lane did not cause it. The styleguide has grown a lot since Task 13, and the host was shared with another lane's Rust build. The test stays enforced, so the CI e2e job is red until the trend step is made cheaper or the budget is re-ruled.

## Consequences

- A change that breaks a web test, a Stockfish-backed test, formatting, the ban lint, the production build or the throttle budget fails CI, not only a local run.
- CI runs longer: images are built per job, and the production lane builds Next. The jobs run in parallel.
- Running the fakes specs and the production specs needs two stacks. Locally that is two projects; the CLAUDE.md limit of two at once holds.
