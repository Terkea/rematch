# ADR-R2-1. Profile engine crates, dependencies and data layout

Date: 2026-11-21
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs after R1's 0031-0038 before merging. Builds on 0030 and 0031.

## Context

Milestone R2 ports the TypeScript profile engine (TS M2, `docs/superpowers/plans/2026-11-21-02-profile-engine.md`) to the Rust workspace of R1. It runs in up to nine parallel lanes while R1 lanes are still editing their own crates. As in R1 (ADR 0031), the lanes must not fight over the workspace manifest, the lockfile, crate module lists or the migration sequence. So one bootstrap task declares every R2 crate, module, dependency and shared type up front, and later tasks only fill in bodies.

## Decision

### Seven crates and why each boundary

| crate | kind | role | boundary reason |
|---|---|---|---|
| `coaching-input` | lib | The M3 `CoachingInput` contract as serde + schemars types, `LIMITS`, `validate()`, generated JSON Schema. | R3 depends on it without pulling in the engine. No sqlx. |
| `profile-core` | lib | All pure chess and statistics logic: board helpers, motifs, features, weakness detectors W1-W12, per-game metrics, strengths, statistics, timeline, KPIs, openings. | Pure and deterministic: no sqlx, no I/O, no unseeded RNG, so the TS test values can be pinned exactly. |
| `refdata` | lib | Storage of reference builds, the opening explorer, band references, reference games and repertoire rows. | The only owner of the reference tables; shared by the `lichess-db` CLI and the worker. |
| `lichess-db` | lib + bin | Operator CLI (R-11): streams a monthly dump (HTTP, then SHA-256 and zstd, then the parser), parses in parallel with rayon, builds the explorer and reference-game set, and fetches Syzygy 3-4-5. | Runs by hand or cron, never in the worker, so its heavy dependencies (rayon, async-compression) stay out of the worker's hot path. |
| `enrich` | lib | Engine-side enrichment: MultiPV, PV completion, the null-move threat probe, Syzygy probes, difficulty providers. | Has its own protocol ids, so `sf19-n300k-v1` evals stay untouched. It also owns the separate tablebase engine set. |
| `scheduler` | lib | Tier selection, fair dequeue on R1's Postgres queue, the 30k-node preview pass, the `schedule-analysis` job, ETA and lag. | Queue policy is kept apart from profile logic. It depends on `analysis`, but `analysis` does not depend on it. |
| `profile` | lib | Integration: the analyse-game hook, reference-game jobs, `build-profile`, and the `CoachingInput` builder. | The only crate that sees everything. `worker` and `cli` depend on it, plus `scheduler`, `enrich` and `refdata`. |

Every library crate dev-depends on `testkit` and on tokio `test-util`. Every module named by a later R2 task is already declared in its crate's `lib.rs` (or the folder's `mod.rs`) and exists as an empty file, so no two lanes edit the same module list. The shared definitions below are final; later tasks add behaviour, not fields:
- `profile-core::{types, constants, bands}`;
- `coaching-input::enums` (`AreaId`, `BandId`, `WeaknessState`);
- `enrich::types` (`TablebaseProbe`, `DifficultyProvider`, `DifficultyInput`, `DifficultyError`);
- the R2 job kinds appended to `queue::kinds`: `schedule-analysis`, `preview-game`, `build-profile`, `analyse-reference-game`, with their payloads, dedupe keys, `PREVIEW_PRIORITY = 0` and `REFERENCE_PRIORITY = 200`.

`chrono` is not used (ADR 0031 amendment). Every timestamp is `time::OffsetDateTime`, so schemars gets no `chrono04` feature. In the contract, timestamp fields are annotated as strings (Task 2).

### New dependencies and the 7-day rule

Every version below was published at least 7 days before 2026-11-21 and is pinned exactly in `[workspace.dependencies]`. Where the newest release was younger, the newest qualifying release is used.

| crate | version | published | licence | use / features |
|---|---|---|---|---|
| schemars | 1.2.3 | 2026-10-12 | MIT | Contract JSON Schema; `derive`, `uuid1` |
| async-compression | 0.4.52 | 2026-11-02 | MIT OR Apache-2.0 | Streamed zstd; `tokio`, `zstd` |
| sha2 | 0.11.1 | 2026-10-12 | MIT OR Apache-2.0 | Dump and tablebase checksums, salted player hashes. Already in the lockfile through sqlx. |
| hex | 0.4.3 | 2021-03-03 | MIT OR Apache-2.0 | Hex digests. Already in the lockfile. |
| rayon | 1.12.0 | 2026-04-14 | MIT OR Apache-2.0 | Parallel PGN parse |
| tokio-util (existing) | 0.7.20 | | MIT | Adds feature `io` (`StreamReader`) |
| tokio (existing) | 1.54.1 | | MIT | Adds feature `fs` (Syzygy files, data dir) |

These pull in 18 new crates in total: the six direct crates (minus sha2 and hex, already locked) and their dependencies. `compression-codecs` is pinned back to 0.4.47 in `Cargo.lock`, because 0.4.48 (2026-11-21) was too new.

Every new licence is on the existing allowlist:
- MIT, `MIT OR Apache-2.0`, Apache-2.0;
- `zstd`, `zstd-safe`: BSD-3-Clause;
- `zstd-sys`: `BSD-3-Clause AND BSD-2-Clause`.

`deny.toml` is unchanged. `scripts/check-crate-age.sh` and `cargo deny --locked check` pass. No statistics crate is added, because Task 11 writes the numerics itself so the TS test values are pinned exactly.

**jsonschema is not added.** The plan listed it as a dev-dependency for the Task 2 parity test, but at 0.61.2 (the newest qualifying release) its `referencing` crate needs `fluent-uri`, which needs `borrow-or-share` (licence MIT-0). MIT-0 is permissive but not on the allowlist, and the allowlist is only widened by an ADR addendum. Two more points:
- In this `deny.toml` configuration, `cargo deny list` and `check` do not report dev-only crates (`wiremock` and `jsonschema` were both absent). So the violation passed `cargo deny`, and was found by listing licences with `cargo metadata`.
- Task 2 checks parity without a JSON Schema validator. It compares the generated schema with the TS-exported schema structurally (property names, `required`, types including `null`, bounds), and round-trips fixtures through serde (`deny_unknown_fields`) and `validate()`. Alternatively, the controller can approve MIT-0 in an addendum and add jsonschema in a coordinator commit.

**schemars `required` handling (ledger ruling Q5), verified at 1.2.3.**
- A plain `Option<T>` field is emitted as `"type": [T, "null"]` and is **not** listed in `required`.
- `#[schemars(required)]` on an `Option<T>` lists it in `required` but **drops `null`** from its type.

The contract needs both: every key must be present (zod `.nullable()`, never `.optional()`), and `null` must be allowed. So Task 2 must not rely on either default. It should add every property to `required` with a schema transform that keeps the `null` type, and the parity test must check this against the TS schema.

### Migrations

R2 owns versions `20261121020000`-`20261121029999` (ledger: R1 owns `…000000`-`…019999`, R3 owns `…030000`-`…039999`). This replaces the `…010000` range written in the R2 plan text.
- Task 3 creates the only R2 migration, `db/migrations/20261121020000_profile_engine.sql`.
- A task that needs another schema change stops and reports. The coordinator then adds `20261121020100_*`, and further ones step by 100.
- R2 migration branches merge only after R1 T19, so on long-lived databases every R1 migration is older.

### Data layout

- `DATA_DIR` is git-ignored (`/data/` in `.gitignore`). It holds the Syzygy tables (`$DATA_DIR/syzygy`, `SYZYGY_PATH`) and any operator scratch.
  - `rust-dev`: `DATA_DIR=/work/data`, `SYZYGY_PATH=/work/data/syzygy`, `MAIA_URL=${MAIA_URL:-}`.
  - `rust-worker` (Task 35): `/data`, mounted read-only.
- The Lichess dump is never stored: it is streamed through SHA-256 and zstd into the parser. Large data is never committed, and test fixtures are at most 50 KB.
- `.env.example` documents `DATA_DIR`, `SYZYGY_PATH` and `MAIA_URL`. An empty `MAIA_URL` means the spread-based difficulty fallback.

### ADR placeholder scheme

R2 ADRs are written as `docs/decisions/ADR-R2-<n>-<slug>.md`, numbered 1-18, each by the task named in the plan. The README lists all 18 rows now (Task 1), and Task 35 finalises the titles. Before merging, the controller renumbers them to the next free numbers after R1's 0031-0038 (expected 0039-0056).

## Alternatives

- **One `profile` crate holding everything.** Rejected. R3 would depend on sqlx and the engine just to read the contract, and the pure logic could no longer be tested without I/O.
- **Each task adds its own dependencies.** Rejected. Parallel lanes would conflict on `Cargo.lock` (ADR 0031).
- **A statistics crate** (`statrs`). Rejected. Pinning the TS reference values exactly needs our own numerics.
- **jsonschema (any feature set).** Rejected for now: MIT-0 via `borrow-or-share` (see above). Its default features would also pull in reqwest, aws-lc-rs and a network resolver.

## Consequences

- Later R2 tasks change no manifest or lockfile. A task that needs a crate or feature stops and reports it.
- `cargo` warns about no unused dependencies (`unused_dependencies = "allow"`, ADR 0031) until the crates are filled in.
- Empty module files compile, so the workspace gate passes from this commit on.
