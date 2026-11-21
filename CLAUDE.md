# chess-improver

TypeScript monorepo (pnpm workspaces + Turborepo, Node 22, ESM only). Packages use the `@ci/*` scope.

## Run

- Install: `pnpm install` (pnpm 12; Node version in `.nvmrc`).
- Stack (Postgres, Redis, worker with Stockfish): `docker compose up -d`. Host ports 55432 (Postgres) and 56379 (Redis) are opt-in via `COMPOSE_FILE=docker-compose.yml:docker-compose.ports.yml` in `.env` (see `.env.example`; keep `COMPOSE_PATH_SEPARATOR=:`). Parallel or CI projects run `COMPOSE_FILE=docker-compose.yml docker compose -p <name> ...`, which publishes no host ports.
- Build / lint / typecheck: `pnpm build`, `pnpm lint`, `pnpm typecheck` (all via `turbo run`).
- Format: `pnpm format` (Biome does both lint and format).

## Test

- All tests: `pnpm test`. One package: `pnpm --filter @ci/chess-core test`.
- Tests use Vitest; they live in `<package>/test/*.test.ts`. Write the failing test first.
- DB tests need Postgres and Redis (compose or CI service containers).

## Rust backend

- Cargo workspace in `backend/` (ADRs 0030, 0031). Build and test only in Docker: `$RUST` means `COMPOSE_FILE=docker-compose.yml docker compose -p ci-<lane> run --rm -e LANE=<lane> rust-dev` (no host ports; `CONTACT_EMAIL` is not needed).
- Once per Docker host (and in CI): `sh scripts/rust-cache-init.sh`. It creates the external volumes `ci-cargo-registry`, `ci-cargo-git`, `ci-sccache` and `ci-crate-age` (cached crates.io publish dates), which every project shares (ADR 0042). Without them compose stops with `external volume "ci-…" not found`.
- `-e LANE=<lane letter>` makes the entrypoint build into its own `/target/<lane>` (default `main`), in the project's `rust_target` volume, seen by cargo as `/lane-target`. Dependencies come from the shared sccache, so a new lane only compiles workspace crates. Tear down with `COMPOSE_FILE=docker-compose.yml docker compose -p ci-<lane> down -v`. That removes the lane's target dir and database but never the shared caches. Never `docker volume rm` the `ci-cargo-*`, `ci-sccache` or `ci-crate-age` volumes during lane cleanup.
- `scripts/check-crate-age.sh` only queries crates.io for versions no lane has cached. It retries 429/5xx with backoff for up to 60 s, then fails closed. `CRATE_AGE_OFFLINE=1` makes no network calls. Its tests: `$RUST sh scripts/test-check-crate-age.sh`.
- `cargo nextest run --workspace` is available in the image as a faster local runner. The gate stays `cargo test`.
- One crate: `$RUST cargo test --locked -p <crate>`. Full gate: `$RUST sh -c "cargo fmt --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo deny --locked check && cargo test --locked --workspace && sh scripts/check-crate-age.sh"`.
- Migrations live in `db/migrations/` (plain SQL, applied by sqlx). They are the schema source of truth.
- Only the coordinator edits `Cargo.toml` dependency lists and `Cargo.lock`. Crates are pinned exactly, at least 7 days old (`scripts/check-crate-age.sh`), and checked by `cargo deny`. Always pass `--locked`.
- DB tests use `#[sqlx::test(migrator = "db::MIGRATOR")]`. With `IN_DOCKER=1`, a missing Postgres or Stockfish fails the test instead of skipping it.
- Use `testkit` only from integration tests in `tests/`, never from `#[cfg(test)]` unit tests: the dev-dependency cycle would compile the crate twice.
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
