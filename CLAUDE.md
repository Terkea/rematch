# chess-improver

TypeScript monorepo (pnpm workspaces + Turborepo, Node 22, ESM only). Packages use the `@ci/*` scope.

## Run

- Install: `pnpm install` (pnpm 12; Node version in `.nvmrc`).
- Stack (Postgres, Redis, worker with Stockfish): `docker compose up -d`.
- Build / lint / typecheck: `pnpm build`, `pnpm lint`, `pnpm typecheck` (all via `turbo run`).
- Format: `pnpm format` (Biome does both lint and format).

## Test

- All tests: `pnpm test`. One package: `pnpm --filter @ci/chess-core test`.
- Tests use Vitest; they live in `<package>/test/*.test.ts`. Write the failing test first.
- DB tests need Postgres and Redis (compose or CI service containers).

## Rust backend

- Cargo workspace in `backend/` (ADRs 0030, 0031). Build and test only in Docker: `$RUST` means `docker compose run --rm rust-dev`.
- Set `LANE=<lane letter>` before `$RUST`, so each lane builds into its own `/target/$LANE` in the shared volume.
- One crate: `$RUST cargo test --locked -p <crate>`. Full gate: `$RUST sh -c "cargo fmt --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo deny --locked check && cargo test --locked --workspace && sh scripts/check-crate-age.sh"`.
- Migrations live in `db/migrations/` (plain SQL, applied by sqlx). They are the schema source of truth.
- Only the coordinator edits `Cargo.toml` dependency lists and `Cargo.lock`. Crates are pinned exactly, at least 7 days old (`scripts/check-crate-age.sh`), and checked by `cargo deny`. Always pass `--locked`.
- DB tests use `#[sqlx::test(migrator = "db::MIGRATOR")]`. With `IN_DOCKER=1`, a missing Postgres or Stockfish fails the test instead of skipping it.
- Use `testkit` only from integration tests in `tests/`, never from `#[cfg(test)]` unit tests: the dev-dependency cycle would compile the crate twice.
## Conventions

- TypeScript `strict` and `noUncheckedIndexedAccess`; relative imports use the `.js` extension.
- New packages copy `packages/chess-core` (package.json, tsconfig.json, tsconfig.build.json).
- Every decision gets an ADR in `docs/decisions/` (next number, add it to that README).
- Commit messages follow Conventional Commits.

## Pointers

- Spec: `docs/superpowers/specs/2026-11-21-chess-improver-design.md`
- Roadmap and plans: `docs/superpowers/`
- Research: `docs/research/`
