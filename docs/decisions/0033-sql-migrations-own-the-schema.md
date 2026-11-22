# 0033. SQL migrations own the schema, Drizzle is generated

Date: 2026-11-21
Status: Accepted. Implements 0030.

## Context

Until R1 the schema lived in `packages/db/src/schema.ts` and Drizzle generated the migrations from it. The Rust backend reads and writes the same Postgres database, so two tools cannot both own the schema. Existing databases (local, and any deployed one) were migrated by Drizzle and carry `drizzle.__drizzle_migrations`.

## Decision

- **`db/migrations/` is the source of truth.** Plain SQL files, applied by sqlx (`db::MIGRATOR`, `db::pool::migrate`) when the worker and the CLI start. Tests get a fresh database from `#[sqlx::test(migrator = "db::MIGRATOR")]`.
- **The baseline is the three Drizzle migrations concatenated** (`20261121000000_m1_baseline.sql`, statement breakpoints removed). Its identity is proved, not assumed: `backend/scripts/check-baseline-schema.sh` migrates one database with drizzle-kit and another with only the baseline file, then diffs `pg_dump --schema-only --no-owner --no-privileges --exclude-schema=drizzle` and a catalogue introspection (columns, constraints, indexes, enum labels, sequences). The expected output is empty with exit code 0.
- **Existing databases are adopted.** `db::baseline::adopt_drizzle_baseline` runs before `migrate`. When `_sqlx_migrations` has no rows and `drizzle.__drizzle_migrations` has exactly three rows, it creates `_sqlx_migrations` with sqlx's own DDL and records the baseline with the checksum embedded in `MIGRATOR`; `migrate` then applies only later migrations. If `_sqlx_migrations` already has rows it does nothing, and a database with no history at all is left to `migrate`. Adoption also checks that `created_at` of the three Drizzle rows equals the known journal timestamps and that every baseline table (`games`, `linked_accounts`, `moves`, `import_runs`, `user_events`, `position_evals`) and the `analysis_status` enum exist. Any other state (for example two Drizzle rows, three rows with a missing table, or tables without any history) is an error that names the state, so a half-migrated database is never guessed at.
- **Migrations run on their own connection.** `db::pool::connect_for_migrations` disables `statement_timeout` (a long index build must not be cut at 30 s) and is the pool to pass to `migrate`. It sets the timeout as a startup parameter, which pgbouncer in transaction mode rejects, so migrations must connect to Postgres directly, not through a pooler.
- **The web app's Drizzle schema is generated** from the SQL migrations (Task 15), so the web app keeps typed queries without owning DDL. `drizzle-kit generate` is no longer run by hand.
- **Version ranges.** `20261121000000` to `20261121009999` are reserved for R1 (baseline, jobs, events, analysis generations). Later milestones, such as M4's Better Auth tables, use later versions.

## Consequences

- One migration history, in one directory, applied by one tool; migrations are never edited after release (sqlx verifies checksums).
- The schema check needs Docker and the TypeScript worker image, so it runs as a script (CI job `baseline-schema`) rather than in `cargo test`.
- Until the Drizzle migration files are deleted (R1 Task 27), changing the schema means writing a new SQL migration and regenerating the Drizzle schema.

## Generated schema and M4 hand-off

- `scripts/db/generate-drizzle-schema.sh` creates a scratch database `schema_pull`, applies every `db/migrations/*.sql` in version order (`psql -1`, `ON_ERROR_STOP`), runs `drizzle-kit pull` (`pnpm --filter @ci/db db:pull`, config `packages/db/drizzle.pull.config.ts`, `_sqlx_migrations` excluded), formats with Biome and drops the database. The output, `packages/db/src/generated/schema.ts` and `relations.ts`, is committed.
- CI job `schema-drift` runs the script and then `git diff --exit-code packages/db/src/generated`. A migration change without regeneration fails it with the command to run. Every lane that adds a migration (T16 `user_events_watermark`, T19, M4) reruns the script as its last step and commits the regenerated files.
- M4 (`apps/web`) imports the generated schema and adds its own tables (Better Auth) only as SQL migrations in `db/migrations/` with versions after `20261121009999`, then reruns the script. Until R1 Task 27 removes `packages/db/src/schema.ts`, the hand-written schema remains the `@ci/db` export; generated enum names differ (see the rename list below); the `$type<>` narrowing is re-applied by the normalisation step.

### Generator safety and normalisation (fix round 1)

**Safety.** The scratch database is `schema_pull_<pid>_<epoch>` (unique per run); the exit trap drops only that database and removes only the scratch directory `packages/db/.pull-tmp-<pid>`. `drizzle-kit pull` writes into that directory (`PULL_OUT`); `schema.ts` and `relations.ts` are copied over the committed files only after pull, normalisation and Biome all succeeded. A failed run leaves the committed output untouched. `drizzle-kit` and `drizzle-orm` are pinned exactly (0.31.11, 0.45.4) so the output cannot change under a range bump.

**Normalisation** (`packages/db/scripts/normalise-generated.mjs`, run by the generator before Biome; each rule fails loudly if its input pattern is absent):

1. All timestamp columns use `mode: "date"` (drizzle-kit emits `"string"`), matching the hand-written schema.
2. `user_events.id` is `bigserial({ mode: "number" })` (drizzle-kit emits `"bigint"`). `jobs.id` stays `bigint`.
3. `position_evals.pv` and `interactive_evals.moves` (R3) defaults are `` sql`'{}'::text[]` `` (drizzle-kit emits `[""]`); the `sql` import is added.
4. Every `.op("..._ops")` opclass is removed: drizzle-kit attaches opclass names from the wrong column (for example `timestamptz_ops` on a uuid column). Postgres uses the default opclass anyway.
5. `$type<ImportCursor>()` on `linked_accounts.import_cursor` and `$type<ImportErrorCode>()` on `import_runs.error_code` are re-applied, and both types are exported from the generated `schema.ts`.
6. Output order is stable: enums first (tables evaluate them eagerly), then tables, each sorted by export name; in `relations.ts` blocks and the entries inside each block are sorted by name.
7. The `relations.ts` import `./schema` becomes `./schema.js` (nodenext).

**Enum rename list** (hand-written export to generated export): `platformEnum` to `platform`, `timeClassEnum` to `timeClass`, `colorEnum` to `color`, `resultEnum` to `result`, `analysisStatusEnum` to `analysisStatus`, `evalSourceEnum` to `evalSource`, `judgementEnum` to `judgement`, `phaseEnum` to `phase`, `importRunStatusEnum` to `importRunStatus`. New: `jobState`. Table exports keep their names. Note `platform`, `color`, `result` and `phase` are generic words and can shadow local variables.

**Import path.** `@ci/db/generated` (and `@ci/db/generated/relations`) export the generated files: `development` condition to `src/generated/*.ts` (ADR 0026), `types`/`default` to `dist/generated/*`; the build emits them. `@ci/db` itself still exports the hand-written `schema.ts`. **Task 27 owns the switch of the main `@ci/db` export to the generated schema**: it removes the hand-written `schema.ts`, repoints `export * from "./schema.js"` at the generated schema and migrates the enum consumers to the new names above. M4 may import `@ci/db/generated` before that.

**What drift detection does NOT cover.** `drizzle-kit pull` does not emit comments, so comment-only migrations (`COMMENT ON ...`) produce no diff; triggers and functions are not represented; some CHECK constraints and partial-index details may not round-trip. Only column, enum, index, key and relation changes reliably show up as drift. The Rust `sqlx` code and the SQL migrations remain the authority for everything else.
