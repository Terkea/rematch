# chess-improver

TypeScript monorepo (pnpm workspaces + Turborepo, Node 22, ESM only). Packages use the `@ci/*` scope.

## Run

- Self-host bootstrap (ADR 0097): `sh scripts/setup.sh [minimal|full|configure|doctor|data|ai|ai-start|ai-stop|ai-status]` writes `.env` (merged, never overwritten), brings the stack up and fetches the data; `pnpm seed:demo [--remove]` loads the fictional `demo.player`. Its tests: `sh scripts/test-setup.sh`.
- Install: `pnpm install` (pnpm 12; Node version in `.nvmrc`).
- Stack (Postgres, Redis, worker with Stockfish): `docker compose up -d`. Host ports 55432 (Postgres) and 56379 (Redis) are opt-in via `COMPOSE_FILE=docker-compose.yml:docker-compose.ports.yml` in `.env` (see `.env.example`; keep `COMPOSE_PATH_SEPARATOR=:`). Parallel or CI projects run `COMPOSE_FILE=docker-compose.yml docker compose -p <name> ...`, which publishes no host ports.
- Build / lint / typecheck: `pnpm build`, `pnpm lint`, `pnpm typecheck` (all via `turbo run`).
- Format: `pnpm format` (Biome does both lint and format).

## Test

- All tests: `pnpm test`. One package: `pnpm --filter @ci/chess-core test`.
- Tests use Vitest; they live in `<package>/test/*.test.ts`. Write the failing test first.
- DB tests need Postgres and Redis. Run the gate in compose, as CI does (ADR 0067): every package but web in the `worker` container (Stockfish), and web in the `web` container (default `BETTER_AUTH_SECRET`). Use `pnpm turbo run build lint typecheck test --concurrency=1`.
- `pnpm format:check` fails on Biome format drift. Run `pnpm format` and commit the result.

## UI

- Load the `design-system` skill (and `chess-ui` for board or graph work) before any UI change. `packages/ui/DESIGN.md` is the law.
- Tokens only: no raw colours, sizes or fonts. `pnpm design:check` (the ban lint) enforces it.
- Before calling a UI change done: `pnpm shots`, `pnpm design:check`, and a critique by a fresh subagent.
- Web stack: `docker compose up -d web` serves `localhost:3100`, and Mailpit (sign-in mail) is at `localhost:48025`. Both need the ports file in `COMPOSE_FILE`.
- e2e: `COMPOSE_FILE=docker-compose.yml:docker-compose.e2e.yml docker compose -p <name> --profile e2e up -d web fakes`, then `... --profile e2e --profile qa run --rm --no-deps qa sh -c "cd apps/web && node node_modules/@playwright/test/cli.js test --project=e2e"`. Put a spec name before `--project`.
- Visual tests: the same `qa` run with `--project='visual-*'`. Baselines are made in that Linux container only.
- Production e2e (what CI runs, ADR 0067): `export E2E_BETTER_AUTH_SECRET=$(openssl rand -hex 32)` (production refuses the compose default), add `docker-compose.e2e-prod.yml` to `COMPOSE_FILE`, `up -d --wait`, and pass `-e E2E_PRODUCTION=1` and `--grep-invert @fakes` to the run.

## Rust backend

- Cargo workspace in `backend/` (ADRs 0030, 0031). Build and test only in Docker: `$RUST` means `COMPOSE_FILE=docker-compose.yml docker compose -p ci-<lane> run --rm -e LANE=<lane> rust-dev` (no host ports).
- Once per Docker host (and in CI): `sh scripts/rust-cache-init.sh`. It creates the external volumes `ci-cargo-registry`, `ci-cargo-git`, `ci-sccache` and `ci-crate-age` (cached crates.io publish dates), which every project shares (ADR 0042). Without them compose stops with `external volume "ci-…" not found`.
- `-e LANE=<lane letter>` makes the entrypoint build into its own `/target/<lane>` (default `main`), in the project's `rust_target` volume, seen by cargo as `/lane-target`. Dependencies come from the shared sccache, so a new lane only compiles workspace crates. Tear down with `COMPOSE_FILE=docker-compose.yml docker compose -p ci-<lane> down -v`. That removes the lane's target dir and database but never the shared caches. Never `docker volume rm` the `ci-cargo-*`, `ci-sccache` or `ci-crate-age` volumes during lane cleanup.
- `scripts/check-crate-age.sh` only queries crates.io for versions no lane has cached. It retries 429/5xx with backoff for up to 60 s, then fails closed. `CRATE_AGE_OFFLINE=1` makes no network calls. Its tests: `$RUST sh scripts/test-check-crate-age.sh`.
- `cargo nextest run --workspace` is available in the image as a faster local runner. The gate stays `cargo test`.
- One crate: `$RUST cargo test --locked -p <crate>`. Full gate: `$RUST sh -c "cargo fmt --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo deny --locked check && cargo test --locked --workspace && sh scripts/check-crate-age.sh"`.
- Migrations live in `db/migrations/` (plain SQL, applied by sqlx). They are the schema source of truth.
- Only the coordinator edits `Cargo.toml` dependency lists and `Cargo.lock`. Crates are pinned exactly, at least 7 days old (`scripts/check-crate-age.sh`), and checked by `cargo deny`. Always pass `--locked`.
- DB tests use `#[sqlx::test(migrator = "db::MIGRATOR")]`. With `IN_DOCKER=1`, a missing Postgres or Stockfish fails the test instead of skipping it.
- Syzygy tables (ADR-R2-12) are only in the main checkout's `data/syzygy`. From a worktree in `.worktrees/<name>`, set `SYZYGY_HOST_DIR=../../data/syzygy` (compose resolves it from the worktree; with `MSYS_NO_PATHCONV=1`), or the real-table tests fail under `IN_DOCKER=1`.
- Maia (ADR-R2-13) is opt-in: add `docker-compose.maia.yml` to `COMPOSE_FILE` (and `docker-compose.maia-ports.yml` for the dev host port). The base file creates one network per project, so parallel lanes don't exhaust Docker's address pools.
- `data/` (git-ignored) holds large local data: the Syzygy tables in `data/syzygy`, never the Lichess dump (it is streamed). `rust-worker` mounts it read-only at `/data` (`DATA_DIR`, `SYZYGY_PATH=/data/syzygy`); from a worktree, create an empty `data/syzygy` there and set `SYZYGY_HOST_DIR`.
- `lichess-db` (operator binary, also in the `rust-worker` image): `build --month YYYY-MM [--explorer] [--band-reference]` builds the explorer or band reference from a streamed, verified dump; `bench` measures the pipeline; `syzygy` fetches the pinned 3-4-5 tables; `resume [--build <id>]` re-enqueues reference games after a crash.
- `cli profile --platform <p> --username <u> --time-class <tc> [--json]` prints the CoachingInput summary (or the whole input); `cli eta --platform <p> --username <u>` the analysis ETA; `cli queue-lag [--json]` the queue lag and scale advice. They only read the database (`docker compose run --rm --entrypoint cli rust-worker ...`). `cli analyse` starts a full worker in-process, so it also requeues every unenriched game in the database; it waits only for the named account's games.
- The `postgres` service keeps `init: true` and an exec-form TCP healthcheck (ADR 0044). Without them, an orphaned `pg_isready` makes Postgres crash-restart mid-run, and tests fail with EOF or "in recovery mode". Regression test: `COMPOSE_FILE=docker-compose.yml sh scripts/test-postgres-orphan-reap.sh ci-<lane>-pg`.
- Use `testkit` only from integration tests in `tests/`, never from `#[cfg(test)]` unit tests: the dev-dependency cycle would compile the crate twice.
- AI needs the `ai-sidecar` service (ADR 0062) and `ANTHROPIC_API_KEY` (a dev `CLAUDE_CODE_OAUTH_TOKEN` works for the sidecar only, never in production); without them reports are computed-only. The worker logs `ai: enabled (sidecar, batches on|off)` or `ai: disabled (<reason>)`. Live AI smoke (spends real tokens, never in CI): with `ANTHROPIC_API_KEY` exported from your `.env` (never typed inline), `AI_LIVE_SMOKE=1 pnpm --filter @ci/ai-sidecar exec vitest run test/live-smoke.test.ts`.
- Dev only, the maintainer's local dev: the coach can run on this machine's signed-in Claude Code, with no key or token (ADR 0062 Amendment 2). Run `pnpm --filter @ci/ai-sidecar start:local` on the host (it reads `.env` and listens on `127.0.0.1:7310`, `/health` says `auth: "local"`). Add `:docker-compose.local-claude.yml` to `COMPOSE_FILE`. With that overlay the Docker sidecar doesn't start, and `rust-worker` calls the host one and publishes its tools on `127.0.0.1:7311`. Production refuses this mode. Anthropic's terms forbid serving other users from a personal login, so any other deployment needs `ANTHROPIC_API_KEY`.
- Timestamps are `time::OffsetDateTime`. JSON that must match TS uses `#[serde(with = "common::js_time::iso")]`.

## Conventions

- TypeScript `strict` and `noUncheckedIndexedAccess`; relative imports use the `.js` extension.
- New packages copy `packages/chess-core` (package.json, tsconfig.json, tsconfig.build.json).
- Every decision gets an ADR in `docs/decisions/` (next number, add it to that README).
- Commit messages follow Conventional Commits.

## Pointers

- Spec: `docs/superpowers/specs/2026-11-21-chess-improver-design.md`
- Roadmap and plans: `docs/superpowers/`
- Research: `docs/research/`
- Releases (images, signing, the GPL source, upgrades): `docs/RELEASING.md`, ADR 0098. Release tests: `node scripts/release/test-release.mjs`; smoke test of built images: `sh scripts/release/smoke.sh <prefix> <version>`.
