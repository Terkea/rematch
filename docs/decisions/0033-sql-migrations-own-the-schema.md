# 0033. SQL migrations own the schema, Drizzle is generated

Date: 2026-11-21
Status: Accepted. Implements 0030.

## Context

Until R1 the schema lived in `packages/db/src/schema.ts` and Drizzle generated the migrations from it. The Rust backend reads and writes the same Postgres database, so two tools cannot both own the schema. Existing databases (local, and any deployed one) were migrated by Drizzle and carry `drizzle.__drizzle_migrations`.

## Decision

- **`db/migrations/` is the source of truth.** Plain SQL files, applied by sqlx (`db::MIGRATOR`, `db::pool::migrate`) when the worker and the CLI start. Tests get a fresh database from `#[sqlx::test(migrator = "db::MIGRATOR")]`.
- **The baseline is the three Drizzle migrations concatenated** (`20261121000000_m1_baseline.sql`, statement breakpoints removed). Its identity is proved, not assumed: `backend/scripts/check-baseline-schema.sh` migrates one database with drizzle-kit and another with only the baseline file, then diffs `pg_dump --schema-only --no-owner --no-privileges --exclude-schema=drizzle` and a catalogue introspection (columns, constraints, indexes, enum labels, sequences). The expected output is empty with exit code 0.
- **Existing databases are adopted.** `db::baseline::adopt_drizzle_baseline` runs before `migrate`. When `_sqlx_migrations` has no rows and `drizzle.__drizzle_migrations` has exactly three rows, it creates `_sqlx_migrations` with sqlx's own DDL and records the baseline with the checksum embedded in `MIGRATOR`; `migrate` then applies only later migrations. If `_sqlx_migrations` already has rows it does nothing, and a database with no history at all is left to `migrate`. Any other state (for example two Drizzle rows, or tables without any history) is an error that names the state, so a half-migrated database is never guessed at.
- **The web app's Drizzle schema is generated** from the SQL migrations (Task 15), so the web app keeps typed queries without owning DDL. `drizzle-kit generate` is no longer run by hand.
- **Version ranges.** `20261121000000` to `20261121009999` are reserved for R1 (baseline, jobs, events, analysis generations). Later milestones, such as M4's Better Auth tables, use later versions.

## Consequences

- One migration history, in one directory, applied by one tool; migrations are never edited after release (sqlx verifies checksums).
- The schema check needs Docker and the TypeScript worker image, so it runs as a script and in CI rather than in `cargo test`.
- Until the Drizzle migration files are deleted (R1 Task 27), changing the schema means writing a new SQL migration and regenerating the Drizzle schema.
