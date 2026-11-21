# R2: Profile Engine in Rust (port of Milestone 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn analysed games into a full player profile in Rust: per-move tactical features, weaknesses W1–W12 with shrunk posteriors and priorities, rating-band references and our own opening explorer built from the CC0 Lichess database by a streaming Rust pipeline, a repertoire tree with verdicts, seven area scores, append-only KPI snapshots, a weakness timeline with change points, a fair priority-driven analysis scheduler with a preview pass, and a `build-profile` job whose output is a `CoachingInput` that satisfies the Milestone 3 contract field for field.

**Architecture:** R2 adds seven crates to R1's cargo workspace `backend/`. `coaching-input` holds the M3 contract as serde + schemars structs (shared with R3). `profile-core` holds all pure chess and statistics logic (no sqlx, no I/O). `refdata` owns the reference tables (builds, explorer, band reference). `lichess-db` is the data pipeline binary: it streams a monthly dump over HTTP through SHA-256 and zstd, parses in parallel with rayon, samples, and writes the explorer and the reference-game set. `enrich` is the engine-side enrichment layer (MultiPV, threat probe, Syzygy, difficulty) with its own protocol ids, so `sf19-n300k-v1` evals stay untouched. `scheduler` selects games by tier, serves users fairly from R1's Postgres queue, runs the 30k-node preview pass and answers ETA and lag queries. `profile` integrates everything: an analyse-game hook, the reference-game jobs, `build-profile`, and the `CoachingInput` builder. The TS plan `docs/superpowers/plans/2026-11-21-02-profile-engine.md` ("TS M2") is the content source: its rulings, thresholds and test values are carried over here and stay binding.

**Tech Stack:** Rust 1.100.0 and R1's pinned crates (tokio, sqlx 0.9 / Postgres 17, reqwest + rustls-ring, cozy-chess, serde, thiserror, tracing, clap), plus schemars 1.x, jsonschema (dev), async-compression (zstd), sha2, hex, rayon. Stockfish 19, Syzygy 3-4-5 tables, Docker Compose. Conditionally Python 3.12 + FastAPI + maia2 for `services/maia` (Task 24).

**Spec:** `docs/superpowers/specs/2026-11-21-chess-improver-design.md` §8, §8a, §8b, §8c, §8d, "8. (cont.)", §12a, §12b, §13a, §14; `docs/research/improvement-engine.md` §4.4 and §6 (normative); ADR 0030. **Content source:** TS M2 (above), including "Contract mapping to M3". **Contract:** `packages/ai/src/input/schema.ts` in worktree `E:\code\chess-improver\.worktrees\m3` (zod, field docs, `LIMITS`). **Builds on:** `docs/superpowers/plans/2026-11-21-R1-rust-core.md` (crate layout, Global Constraints, queue/events/db APIs). In this plan "R1 Tn" is an R1 task and "Task n" is an R2 task.

## Global Constraints

Every R1 Global Constraint applies unchanged. Repeated here because each task needs them:

- **Containers only.** Every cargo command runs as `$RUST <cmd>` = `docker compose run --rm rust-dev <cmd>` (workdir `/work/backend`). Each lane exports `LANE=r2-<lane>` (target dir `/target/r2-<lane>`). Ports bind to `127.0.0.1` only; never touch other projects' containers (`velvet_lab_db` on 5432).
- **Toolchain** `1.100.0` (`backend/rust-toolchain.toml`), edition 2024, `unsafe_code = "forbid"`.
- **Lockfile and manifests:** Task 1 adds every R2 dependency to `[workspace.dependencies]` and every R2 crate's `Cargo.toml`, and commits `Cargo.lock`, as the single coordinator commit on the base branch (R1 lanes rebase onto it). No later R2 task edits a `Cargo.toml` dependency list or `Cargo.lock`; a task that needs a crate stops and reports it. **Supply chain:** every added crate version is at least 7 days old on crates.io at the time of Task 1, pinned exactly (`=x.y.z`), and `cargo deny check` is clean.
- **Licences:** only MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib (and combinations). No copyleft crates. Never widen `deny.toml` silently. Licensing §14 as TS M2: our own code from documented rules; nothing from `cook.py`, `lila-openingexplorer`, `lichess-org/database` scripts, chessops or chessground; data only from CC0 Lichess sources; Maia only if Task 24's licence check passes.
- **Migrations:** R2 owns versions `20261121010000`–`20261121019999`. Task 1 reserves the names; Task 3 creates `db/migrations/20261121010000_profile_engine.sql`, the only R2 migration. A task needing another schema change stops and reports (the coordinator adds `20261121010100_*`). R2 migration branches merge only after R1 T19 (so every R1 migration is older on long-lived databases; `#[sqlx::test]` databases are unaffected). Generated Drizzle files (`packages/db/src/generated/`) are never hand-merged: after a merge, rerun `scripts/db/generate-drizzle-schema.sh` and commit.
- **SQL style:** runtime `sqlx::query`/`query_as` with `#[derive(sqlx::FromRow)]`, no `query!`; enums bound as text with casts (`$1::band`) and read as `col::text`. `profile-core` and `coaching-input` never depend on sqlx.
- **Tests:** DB tests are `#[sqlx::test(migrator = "db::MIGRATOR")]`. With `IN_DOCKER=1` a test that needs Postgres, Stockfish or Syzygy files **fails** (panics), never skips. **No live network in tests**: HTTP is `wiremock`, fixtures are recorded or generated in the test. Committed test fixtures are ≤ 50 KB each. Timing assertions use ≥ 3× bounds or `testkit::wait_for`.
- **Analysis protocol unchanged:** `ANALYSIS_PROTOCOL = "sf19-n300k-v1"` (300 000 nodes, MultiPV 1, no Threads/Hash) for `position_evals`. Enrichment is versioned separately: `ENRICHMENT_PROTOCOL = "enr-v1"`, `MULTIPV_PROTOCOL = "sf19-n300k-mpv3-v1"`, `THREAT_PROTOCOL = "sf19-n50k-null-v1"`, `PREVIEW_PROTOCOL = "sf19-n30k-preview-v1"`. Tablebase probes use a **separate** engine set; the main pool never sets `SyzygyPath`.
- **Thresholds** (improvement-engine §6.2): eligible = `|win_before − 50| < 40`; decisive = `delta_w ≥ 15`; inaccuracy-grade = `delta_w ≥ 5`; critical = MultiPV best − second ≥ 15 W%; material P1 N3 B3 R5 Q9.
- **Bands** on the Lichess-equivalent scale: B0 < 900, B1 900–1299, B2 1300–1699, B3 1700–1999, B4 2000–2299, B5 ≥ 2300. chess.com ratings map through `CHESSCOM_TO_LICHESS_OFFSET` (provisional). Ratings from different platforms never share a series.
- **Intervals** are 80% (`Z80 = 1.2815515655446004`): Wilson for rates, bootstrap (1000 resamples) for means, Beta quantiles for posteriors, 10 000 Monte Carlo draws for P_imp.
- **Determinism:** `profile-core` never uses an unseeded RNG; every stochastic function takes a seed (`Rng::new(seed)`, mulberry32). All arithmetic is `f64`.
- **Data handling:** large data is never committed and the Lichess dump is **never stored** (streamed: HTTP → SHA-256 + zstd → parser). Syzygy files live under the git-ignored `data/` (`DATA_DIR`: `/work/data` in `rust-dev`, `/data` read-only in `rust-worker`). Dump usernames are stored only as `sha256(salt ‖ lower(name))` with a per-build in-memory salt.
- **State and events:** state lives in Postgres; jobs are enqueued after commit; events go through `events::publish` after commit.
- **Per-stage timing:** every new job kind writes one `job_metrics` row (`db::stage_metrics::record_stage_metrics`, Task 3) with `total_ms` and a `stages` map, consistent with R1 ADR 0036.
- **ADRs:** R2 ADRs are written as `docs/decisions/ADR-R2-<n>-<slug>.md` with placeholder numbers `ADR-R2-1` … `ADR-R2-18` (assigned in the task that writes each). The controller renumbers them to the next free numbers after R1's 0031–0038 (expected 0039–0056) before merging. Only Task 1 and Task 35 edit `docs/decisions/README.md`.
- **Commits:** Conventional Commits, each ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Lichess-eval games have empty PVs** (`pv: []`), so motif tags and refutation lines would be empty and W1/W3 silently undercounted. Enrichment fetches a PV for critical plies. Test: Task 25 `refutation_pv_falls_back_to_engine_when_cached_pv_is_empty`.
2. **A thin or empty account must still produce a contract-valid `CoachingInput`.** 0–29 analysed games, no reference build, no rating: the contract still requires `band`, `windowStart`/`windowEnd` and exactly seven `areaScores`, and forbids `bandMedian: null` on a weakness. Expected: "insufficient data" / "no benchmark" with have/need counts, validating against the TS schema, never a crash. Tests: Task 34 `coaching_input_for_account_with_0_analysed_games_validates`, Task 12 `no_reference_gives_priority_0`, Task 15 `area_with_too_little_data_reports_have_and_need`.
3. **Out-of-range or oversize contract values** (a signed metric such as `eval_trend_15_35 = −8`, a 70-char line key, 31 opening lines, a 20-move PV) are rejected by M3's strict schema. The builder must map, truncate or omit by the documented rules, never emit an invalid input. Tests: Task 34 `builder_respects_limits_and_unit_ranges`, Task 2 `validate_rejects_value_outside_unit_range`.
4. **A dump whose SHA-256 does not match** (detected only at the end of a 30 GB stream) must never activate the partial explorer or reference set. Test: Task 20 `sha_mismatch_marks_build_failed_and_keeps_previous_active`.
5. **Games without clocks** (daily, correspondence) leave every W9 metric absent and out of denominators, never counted as zero. Tests: Task 8 `no_clocks_w9_keys_absent`, Task 15 `time_trouble_rate_counts_only_clocked_games`.

Also pinned by owned tests: illegal null-move FENs (Task 4 `null_move_fen_returns_none_in_check`, Task 25 `threat_probe_skipped_when_in_check`), Maia down or tablebase files missing (Task 25 `difficulty_provider_failure_yields_null_difficulty`, Task 23 `missing_tablebase_files_returns_none`), one heavy user starving newcomers (Task 27 `newcomer_gets_first_10_before_anyone_gets_11th`), preview evals leaking into stats (Task 28 `preview_never_writes_position_evals_or_game_metrics`).

## Rulings

Carried from TS M2 (unchanged): strengths are part of the input (controller ruling); internal `WeaknessKey` maps to contract `id` + `variant`; `CandidateCause.kind` uses M3's names; M2 emits no `focus`; changes without an onset and opening lines with `verdict = None` are omitted; T_win fixed at 85; tablebase-eligible = ≤ 5 men; fatigue = 0; W9d and W12 are continuous and never ranked.

New in R2 (each recorded in the ADR of its task):

- **R-1 Contract `Strength` is exactly `{area, label, metricKey, exemplars}`** as in the M3 worktree schema (strict). TS M2's extra `StrengthEntry` fields (`kind`, `weaknessId`, `bandPercentile`, `n`) stay internal (`StrengthCandidate`) and are used only for selection and ordering.
- **R-2 Metrics in the contract** are catalogue rows with a non-null `area` and a `contract_unit`. Rows without an area (`rating`, `perf_vs_expected`, `accuracy`) and rows whose value can be negative or has no contract unit (`eval_trend_15_35`, `session_accuracy_decline`, `critical_time_ratio`, `book_exit_ply`) are internal only. Metric `value`/`low`/`high` are the raw values clamped to the unit range; `bandPercentile` is computed from `value_adjusted`. Rows with `value = None` are omitted.
- **R-3 `weaknesses`** contains only estimates with `reason = ok` (benchmark present and ≥ 30 games), because the contract requires non-null `bandMedian` and `bandPercentile`. `events`/`opportunities` are the unweighted counts inside the 6-month window.
- **R-4 Band fallback:** band from the latest player rating (`to_lichess_equivalent`); else from the mean opponent rating of the last 20 games; else `B2`.
- **R-5 Empty window:** with 0 analysed games, `windowStart = windowEnd = generatedAt` date.
- **R-6 Limits:** the builder truncates every list to `LIMITS` by its own order (weaknesses by rank, metrics by catalogue order, openings by games desc, changes by onset desc, PVs to 16 plies, tags to 12), and `CoachingInput::validate` runs before returning; a validation failure is a bug (`BuildError::Contract`).
- **R-7 Ids:** `ExemplarPosition.gameId` is the **platform** game id (≤ 64 chars). `line_key` is `"{color}:{slug}"` with the slug cut to 40 chars at a `-` boundary plus `-` and 6 hex chars of `sha256(full_slug)` when cut, so every series key fits 64 chars. `fromState = None` (first row) maps to `not_an_issue`.
- **R-8 Scores:** exemplar `evalBefore`/`evalAfter` and `avgEvalAtOpeningEnd` are White-POV `{cp}`/`{mate}`. `avgEvalAtOpeningEnd` is the mean White-POV `score_to_cp` (mate = ±1000) at the last opening ply over the line's analysed games, rounded to an integer.
- **R-9 Dump verification is whole-file.** With `--max-games`, the pipeline stops parsing but keeps hashing to the end of the stream; a build activates only after the SHA-256 matches. `--verify none` exists for development; such a build gets status `unverified` and is never `active`.
- **R-10 One pass over the dump:** explorer aggregation and reference-player selection share one stream. Reference players are chosen by a per-cell **bottom-k sketch** on the salted player hash (oversample 20×), not TS M2's two-pass scan, because the dump is not stored.
- **R-11 `lichess-db` is an operator CLI** (run quarterly by hand or cron, ADR), not a worker job. Per-game reference analysis runs on the worker as `analyse-reference-game` jobs at priority 200 (below every user tier).
- **R-12 Fair scheduling** orders queued analyse-family jobs by `(tier = priority / 2, slice, joined_at, priority, run_at, id)` with `slice = (served + rank − 1) / 5`, where `served` = the account's games with `analysis_started_at` in the last hour and `joined_at = linked_accounts.created_at` (users exist only from M4).
- **R-13 Preview evals** live only in `game_previews` (never `position_evals`, `moves`, `game_metrics` or series). The full pass deletes the preview row in its persist transaction.

## Contract mapping to M3

`coaching_input::CoachingInput` serialises (serde `rename_all = "camelCase"`, `deny_unknown_fields`) to exactly the M3 zod `CoachingInput`. Every object has the zod object's fields, names, nullability and bounds; every `Option` field is present as `null` (never skipped). `schemars` generates the schema, annotated with the same `maxLength`/`maxItems`/`minimum`/`maximum`. `CoachingInput::validate()` re-implements the zod refinements that JSON Schema cannot express (FEN validity, side to move vs ply parity, `low ≤ high`, `intervalLow ≤ posteriorRate ≤ intervalHigh`, `events ≤ opportunities`, unique ranks, each area exactly once, `onsetFrom ≤ onsetDate ≤ onsetTo`, `analysedGames ≤ importedGames`, `windowStart ≤ windowEnd`, unit ranges).

| R2 internal | Contract field | Rule |
|---|---|---|
| `WeaknessKey` (`W3.shallow`, `W9a`) | `RankedWeakness.id` + `variant` | `W3.shallow` → (`W3`, `"shallow"`); `W9a` → (`W9a`, null); `W1` → (`W1`, null) |
| `SeriesKey` | `TimelineChange.seriesKey` + `weaknessId` | weakness key → its `id`; `motif.missed:*` → `W3`; `motif.allowed:*` → `W1`; `opening:*`, `endgame:*` → null |
| `MotifTag` | `ExemplarPosition.tags` | snake_case (`hanging_piece`, `back_rank_mate`, `mate_in_2`) |
| `LineSummary.kind` | `OpeningVerdict.side` | `chosen`/`faced` |
| `WeaknessEstimate.posterior.mean`, `.interval` | `posteriorRate`, `intervalLow`/`intervalHigh` | |
| `WeaknessEstimate.excess` | `excessEplPerGame` | |
| history row without onset | — | omitted |
| `verdict = None` (n < 8) | — | omitted |
| `StrengthCandidate` | `Strength` | R-1 |

## Lanes, waves and dependencies

Each lane has its own worktree and branch `r2/<lane>`, based on the Task 1 commit (which itself sits on R1's base branch after R1 T1).

| Lane | Scope (disjoint files) | Tasks |
|---|---|---|
| 0 bootstrap | all R2 `Cargo.toml`s, `Cargo.lock`, crate skeletons, `profile-core/src/{types,constants,bands}.rs`, `coaching-input/src/enums.rs`, `queue/src/kinds.rs` (append), compose env, `.gitignore`, ADR index rows | 1 |
| K contracts | `backend/crates/coaching-input/**` (except `enums.rs`), `backend/fixtures/coaching-input/**`; `db/migrations/20261121010000_profile_engine.sql`, `db/src/{games.rs,profile_rows.rs,stage_metrics.rs,lib.rs}` | 2, 3 |
| P1 detectors | `profile-core/src/{board,motifs,features,weakness}/**`, `chess-core/src/board.rs` (append only), `profile-core/tests/{board,motifs,features,episodes,detectors,game_metrics,strengths}.rs`, `profile-core/tests/common/**` | 4 → 5 → 6; 7 (after 4); 8 (after 6, 7) → 9; 10 (after 6, 7) |
| P3 statistics | `profile-core/src/{stats,timeline,kpi}/**`, their tests | 11 → 12, 13, 15; 14 (after 12, 13) |
| P2 openings | `profile-core/src/openings/**`, `profile-core/data/**`, `backend/crates/refdata/**` | 16; 17; 18 (after 11, 16, 17) |
| D data pipeline | `backend/crates/lichess-db/**`, `backend/fixtures/lichess-db/**`, `docs/research/r2-lichess-db-benchmark.md` | 19 → 20, 21, 22 |
| E engine-side | `backend/crates/enrich/**`, `services/maia/**`, compose `maia` service | 23; 24; 25 (after 4) |
| S scheduler | `backend/crates/scheduler/**`, `queue/src/{strategy.rs,runner.rs,queue.rs}`, `analysis/src/jobs/{import,reconcile}.rs` | 26; 27 → 28, 30; 29 (after 26, 27) |
| I integration | `backend/crates/profile/**`, `analysis/src/{hook.rs,jobs/analyse.rs,lib.rs}`, `events/src/store.rs`, `worker/**`, `cli/**`, Dockerfile, compose `rust-worker`, CLAUDE.md, ADR index | 31 → 32, 33 → 34 → 35 |

**R1 prerequisites per task** (an R2 task can start as soon as these R1 tasks and its R2 predecessors are merged into its lane's base):

| R2 task | R1 prerequisites | R2 prerequisites |
|---|---|---|
| 1 | T1 | — |
| 2 | T3 (FEN check via `Position::from_fen`) | 1 |
| 3 | T12 (db crate, `games.rs`) | 1 |
| 4 | T3 | 1 |
| 5 | T3 | 4 |
| 6, 7 | T5 (`build_move_records` for fixtures) | 5 (6), 4 (7) |
| 8, 9, 10 | T5 | 6, 7 (8, 10), 8 (9) |
| 11–15 | T1 | 1, then as lane |
| 16 | T3 | 1 |
| 17 | T12 | 3 |
| 18 | T12 | 11, 16, 17 |
| 19 | T2 (`classify_time_control`) | 1 |
| 20 | T3, T12 | 17, 19 |
| 21 | T4 (`parse_pgn`), T13 (`enqueue`) | 17, 19 |
| 22 | T1 | 19 |
| 23 | T7 (`UciEngine` with `syzygy_path`), T12 | 3 |
| 24 | T1 | 1 |
| 25 | T7, T12, T18 (`FakeAnalyser`) | 3, 4 |
| 26 | T1 | 1 |
| 27 | T13, T14 | 3 |
| 28 | T5, T7, T16, T18 | 27 |
| 29 | T19, T20, T21 | 26, 27 |
| 30 | T19 (`job_metrics`) | 27 |
| 31 | T19 | 9, 10, 16, 17, 25 |
| 32 | T19 | 12, 15, 21, 31 |
| 33 | T16 | 14, 15, 18, 29, 31 |
| 34 | — | 2, 33 |
| 35 | T22, T23 | all |

| Wave | R2 tasks (parallel within a wave) | Can start during |
|---|---|---|
| W0 | 1 | R1 W1 (right after R1 T1) |
| W1 | 2, 3, 4, 11, 16, 19, 24, 26 (8 agents) | R1 W1–W2 (needs R1 T2, T3, T12 as listed) |
| W2 | 5, 7, 12, 13, 15, 17, 22, 23, 27 (9 agents) | R1 W2–W3 |
| W3 | 6, 14, 18, 20, 21, 25, 28, 30 | R1 W3–W4 |
| W4 | 8, 10, 29 | after R1 W4 |
| W5 | 9 | |
| W6 | 31 | |
| W7 | 32, 33 | |
| W8 | 34 | |
| W9 | 35 | after R1 W6 (T23) |

Task counts per lane: 0: 1 · K: 2 · P1: 7 · P3: 5 · P2: 3 · D: 4 · E: 3 · S: 5 · I: 5 — 35 tasks.

---

### Task 1: R2 bootstrap — dependencies, crate skeletons, shared profile types

**Files:**
- Modify: `backend/Cargo.toml` (`[workspace.dependencies]`, members already `crates/*`), `backend/Cargo.lock`, `backend/deny.toml` (only if a new licence *expression* of allowed licences needs listing; never a new licence), `backend/crates/queue/src/kinds.rs` (append only), `docker-compose.yml` (`rust-dev` env only), `.gitignore` (`/data/`), `.env.example` (`DATA_DIR`, `SYZYGY_PATH`, `MAIA_URL`), `docs/decisions/README.md` (rows `ADR-R2-1` … `ADR-R2-18`)
- Create crates, each with `Cargo.toml`, `src/lib.rs` declaring every module named in its tasks, and empty module files: `backend/crates/{coaching-input,profile-core,refdata,enrich,scheduler,profile}`, `backend/crates/lichess-db` (lib + `src/main.rs` with a clap skeleton whose subcommands are added by Tasks 19–22)
- Create: `backend/crates/profile-core/src/{types.rs,constants.rs,bands.rs}`, `backend/crates/coaching-input/src/enums.rs`, `backend/crates/enrich/src/types.rs`
- Create: `docs/decisions/ADR-R2-1-profile-crates-and-data-layout.md`
- Test: `backend/crates/profile-core/tests/contracts.rs`

**Interfaces:**
- Produces (final in this task; later tasks implement behaviour, not these definitions):

```rust
// coaching-input/src/enums.rs (serde snake_case/lowercase exactly as the zod enums; schemars derive)
pub enum AreaId { Openings, Middlegame, Endgame, Tactics, Calculation, Time, Mindset }
pub enum BandId { B0, B1, B2, B3, B4, B5 }
pub enum WeaknessState { NotAnIssue, Emerging, Active, Improving, Resolved, Relapsed }

// profile-core/src/types.rs
pub use coaching_input::{AreaId as Area, BandId as Band, WeaknessState};
pub const AREAS: [Area; 7]; // openings, middlegame, endgame, tactics, calculation, time, mindset
pub enum MotifTag { HangingPiece, Fork, Pin, Skewer, DiscoveredAttack, Deflection, Sacrifice, Promotion,
    Mate, MateIn1, MateIn2, MateIn3, MateIn4Plus, MatingNet, BackRankMate, SmotheredMate }
//   Ord = declaration order; as_str() camelCase ("hangingPiece", "mateIn4Plus"); contract_tag() snake_case
//   ("hanging_piece", "mate_in_4_plus"); ALL: [MotifTag; 16]
pub enum WeaknessKey { W1, W2, W3Shallow, W3Deep, W4Missed, W4Allowed, W5, W6, W7Traps, W7Prep,
    W8Basics, W8Advanced, W9a, W9b, W9c, W9d, W10, W11, W12 }
//   as_str() "W3.shallow" etc.; FromStr; is_binomial() = not W9d/W12; contract_id() -> (&str, Option<&str>)
pub enum EndgameType { Pawn, Rook, Minor, Queen, Mixed }       // as_str lowercase
pub enum SeriesKey { Weakness(WeaknessKey), MotifMissed(MotifTag), MotifAllowed(MotifTag),
    Opening(String /* line_key */), Endgame(EndgameType) }      // Display/FromStr: "W1", "motif.missed:fork", "opening:white:italian-game", "endgame:rook"
pub type Wdl = i8;                                               // -1 | 0 | 1
pub struct Swing { pub gain: f64, pub depth: Option<u32>, pub mate_in: Option<u32> }
pub struct LineAnalysis { pub swing: Swing, pub tags: Vec<MotifTag> }
pub struct MultipvLine { pub uci: String, pub score: EngineScore /* mover POV */, pub pv: Vec<String> }
pub struct ThreatProbe { pub null_fen: String, pub pv: Vec<String>, pub score: EngineScore /* opponent POV */ }
pub struct Difficulty { pub p_best: f64, pub p_ok: f64, pub model: String }
pub struct RawMoveFeatures { pub ply: u32, pub multipv: Option<Vec<MultipvLine>>, pub best_pv: Vec<String>,
    pub refutation_pv: Vec<String>, pub threat: Option<ThreatProbe>,
    pub tb_wdl_before: Option<Wdl>, pub tb_wdl_after: Option<Wdl> /* both mover POV */, pub difficulty: Option<Difficulty> }
pub struct MoveFeatures { pub is_critical: Option<bool>, pub multipv_gap: Option<f64>, pub playable: Option<u32>,
    pub in_top2: Option<bool>, pub best: Option<LineAnalysis>, pub refutation: Option<LineAnalysis>,
    pub threat: Option<LineAnalysis>, pub threat_existed: Option<bool>, pub threat_addressed: Option<bool> }
pub struct EnrichedMove { pub rec: MoveRecord, pub fen_after: String, pub score_before: EngineScore,
    pub score_after: EngineScore /* mover POV */, pub raw: Option<RawMoveFeatures>, pub f: MoveFeatures }
pub enum PlatformId { Lichess, Chesscom }
pub struct GameContext { pub game_id: Uuid, pub platform_game_id: String, pub platform: PlatformId,
    pub played_at: DateTime<Utc>, pub time_class: TimeClass, pub base_seconds: Option<u32>,
    pub increment_seconds: Option<u32>, pub rated: bool, pub player_color: Color, pub player_rating: Option<i32>,
    pub opponent_rating: Option<i32>, pub result: String, pub score: Option<f64> /* player POV 1|0.5|0 */,
    pub termination: Option<String>, pub accuracy: Option<f64>, pub protocol: String }
pub struct AnalysedGame { pub ctx: GameContext, pub moves: Vec<EnrichedMove> /* both colours, ply order */,
    pub book_exit_ply: Option<u32>, pub opening_line_key: Option<String> }
pub struct BinomialObs { pub events: u32, pub opportunities: u32, pub epl: f64, pub exemplar_plies: Vec<u32> }
pub struct SeriesObs { pub series_key: SeriesKey, pub events: u32, pub opportunities: u32, pub epl: f64 }
pub struct GameObservation { pub game_id: Uuid, pub binomial: BTreeMap<WeaknessKey, BinomialObs>,
    pub w9d_rho: Option<f64>, pub series: Vec<SeriesObs> }
pub enum StrengthKind { OnlyMoveFound, TacticFound, Conversion, Defence }   // serde snake_case
pub struct StrengthPly { pub kind: StrengthKind, pub ply: u32 }
pub struct SessionInfo { pub game_id: Uuid, pub session_id: u32, pub index_in_session: u32 /* 1-based */,
    pub after_loss: bool, pub residual: Option<f64> }
/// Per-game chess metrics, version 1, stored as game_metrics.metrics jsonb (serde camelCase).
/// Field list exactly TS M2 Task 1 `GameMetrics`, plus `opening.evalAtOpeningEndWhiteCp: Option<i32>` (R-8).
/// Named PlayerGameMetrics to avoid confusion with R1's analysis::metrics::GameMetrics (timing).
pub struct PlayerGameMetrics { pub version: u32, /* ... */ pub strengths: Vec<StrengthPly> }
pub enum BaseMetric { /* the 36 catalogue ids of Task 15, as_str() e.g. "accuracy.opening" */ }
pub enum MetricId { Base(BaseMetric), MotifMissRate(MotifTag), Weakness(WeaknessKey), Area(Area) } // "motif_miss_rate:fork", "weakness:W1", "area:openings"
pub struct BandReferenceStat { pub metric: MetricId, pub median: f64, pub p25: f64, pub p40: f64, pub p75: f64,
    pub pooled_rate: Option<f64>, pub epl_per_event: Option<f64>, pub opps_per_game: Option<f64>,
    pub opp_slope_per_100: f64, pub n_players: u32, pub n_games: u32, pub low_sample: bool }
pub type BandReference = BTreeMap<MetricId, BandReferenceStat>;

// enrich/src/types.rs
pub trait TablebaseProbe: Send + Sync { fn max_pieces(&self) -> u32;
    fn probe<'a>(&'a self, fen: &'a str) -> BoxFuture<'a, Option<Wdl>> /* side-to-move POV */; }
pub struct DifficultyInput { pub fen: String, pub elo_self: Option<i32>, pub elo_oppo: Option<i32>,
    pub lines: Option<Vec<(String /* uci */, f64 /* win% */)>>, pub best_uci: Option<String> }
#[derive(thiserror::Error, Debug)] pub enum DifficultyError { #[error("difficulty service unavailable: {0}")] Unavailable(String) }
pub trait DifficultyProvider: Send + Sync { fn model(&self) -> &str;
    fn estimate<'a>(&'a self, inputs: &'a [DifficultyInput]) -> BoxFuture<'a, Result<Vec<Option<(f64 /* p_best */, f64 /* p_ok */)>>, DifficultyError>>; }

// queue/src/kinds.rs (appended)
pub const KIND_SCHEDULE: &str = "schedule-analysis";      // dedupe "schedule:<account>"
pub const KIND_PREVIEW: &str = "preview-game";            // dedupe "preview:<game>"
pub const KIND_BUILD_PROFILE: &str = "build-profile";     // dedupe "profile:<account>:<time_class>"
pub const KIND_REFERENCE_GAME: &str = "analyse-reference-game"; // dedupe "refgame:<id>"
pub struct SchedulePayload { pub account_id: Uuid, pub plan: Option<String> /* "free" | "pro" */ }
pub struct PreviewPayload { pub game_id: Uuid }
pub struct BuildProfilePayload { pub account_id: Uuid, pub time_class: String }
pub struct ReferenceGamePayload { pub reference_game_id: Uuid }
pub fn schedule_key(a: Uuid) -> String; pub fn preview_key(g: Uuid) -> String;
pub fn profile_key(a: Uuid, tc: &str) -> String; pub fn refgame_key(id: Uuid) -> String;
pub const PREVIEW_PRIORITY: i16 = 0; pub const REFERENCE_PRIORITY: i16 = 200;
```

- `profile-core/src/constants.rs`: `N0`, `MIN_EVIDENCE`, `TRAINABILITY`, `RANKABLE`, `WEAKNESS_AREA`, `WEAKNESS_LABEL`, `EPL_PRECEDENCE`, `CHESSCOM_TO_LICHESS_OFFSET` with exactly TS M2 Task 1's values (N0: W1 300, W2 100, W3.* 60, W4.* 40, W5 30, W6 30, W7.* 40, W8.* 80, W9a 30, W9b 40, W9c 60, W9d 20, W10 600, W11 300, W12 60; MIN_EVIDENCE: W1/W2/W11 1000, W3.* 40, W5/W6 15, W8.* 50, W9a 20, W10 1500, else N0; TRAINABILITY verbatim from improvement-engine §6.4 with the sub-key rules of TS M2; offsets bullet 250, blitz 250, rapid 350, classical 300, daily 300). As functions `n0(k) -> f64`, `min_evidence(k) -> f64`, `trainability(k, band) -> f64`, `weakness_area(k) -> Area`, `weakness_label(k) -> &'static str`.
- `bands.rs`: `band_of(lichess_eq: f64) -> Band`, `to_lichess_equivalent(p: PlatformId, tc: TimeClass, rating: Option<i32>) -> Option<i32>`, `rating_target(rating: Option<i32>) -> Option<i32>` (+200).

- [ ] **Step 1: Choose and verify the versions.** For each new crate below, take the newest release that is at least 7 days old (`https://crates.io/api/v1/crates/<name>` → `versions[].created_at`), check its licence and its new transitive crates' licences, and record the table in ADR-R2-1:

| crate | use | features |
|---|---|---|
| schemars 1.x | contract JSON Schema | `derive`, `chrono04`, `uuid1` |
| jsonschema (dev) | parity validation | `default-features = false` (no HTTP resolver) |
| async-compression | streamed zstd | `tokio`, `zstd` |
| sha2, hex | dump and tablebase checksums | |
| rayon | parallel PGN parse | |
| tokio-util (existing) | `io::StreamReader` | add feature `io` |

  No statistics crate: numerics are our own (Task 11) to pin the TS test values exactly.
- [ ] **Step 2: Declare the crate graph** (all of it, now):
  - `coaching-input` → chess-core, serde, serde_json, schemars, uuid, chrono, thiserror (dev: jsonschema);
  - `profile-core` → chess-core, coaching-input, cozy-chess, serde, serde_json, uuid, chrono, sha2, hex, thiserror;
  - `refdata` → db, profile-core, sqlx, uuid, chrono, serde_json, thiserror, tracing;
  - `lichess-db` (lib + bin) → chess-core, profile-core, refdata, db, queue, reqwest, rustls, tokio, tokio-util, futures, bytes, async-compression, sha2, hex, rayon, uuid, chrono, serde, serde_json, clap, anyhow, thiserror, tracing, tracing-subscriber (dev: wiremock);
  - `enrich` → chess-core, engine, profile-core, db, sqlx, reqwest, tokio, futures, serde, serde_json, sha2, hex, thiserror, tracing (dev: wiremock);
  - `scheduler` → chess-core, engine, db, queue, events, analysis, profile-core, sqlx, tokio, tokio-util, futures, uuid, chrono, serde, serde_json, thiserror, tracing;
  - `profile` → all of the above plus events, sqlx, tokio, tokio-util, futures, uuid, chrono, serde, serde_json, thiserror, tracing;
  - `worker` and `cli` additionally → profile, scheduler, enrich, refdata.
  Every library crate dev-depends on `testkit` and tokio `test-util`.
- [ ] **Step 3: Write the failing test** `profile-core/tests/contracts.rs`:

```rust
#[test] fn bands_and_constants_match_ts_m2() {
    let r = [899., 900., 1299., 1300., 1700., 1999., 2000., 2299., 2300.].map(band_of);
    assert_eq!(r, [B0, B1, B1, B2, B3, B3, B4, B4, B5]);
    assert_eq!(to_lichess_equivalent(Lichess, Blitz, Some(1500)), Some(1500));
    assert_eq!(to_lichess_equivalent(Chesscom, Rapid, Some(1000)), Some(1350));
    assert_eq!(to_lichess_equivalent(Chesscom, Rapid, None), None);
    assert_eq!(rating_target(Some(1450)), Some(1650));
    assert_eq!(trainability(W1, B0), 1.0); assert_eq!(trainability(W10, B0), 0.0);
    assert_eq!(trainability(W7Prep, B5), 1.0); assert_eq!(trainability(W3Deep, B0), 0.2);
    assert_eq!(trainability(W12, B3), 0.0);
    assert_eq!(n0(W1), 300.0); assert_eq!(min_evidence(W1), 1000.0); assert_eq!(min_evidence(W4Missed), 40.0);
    assert!(!RANKABLE.contains(&W12) && !RANKABLE.contains(&W9d)); assert_eq!(AREAS.len(), 7);
}
#[test] fn key_strings_round_trip() {
    for k in WeaknessKey::ALL { assert_eq!(k.as_str().parse::<WeaknessKey>().unwrap(), k); }
    assert_eq!(W3Shallow.contract_id(), ("W3", Some("shallow"))); assert_eq!(W9a.contract_id(), ("W9a", None));
    assert_eq!("motif.missed:fork".parse::<SeriesKey>().unwrap(), SeriesKey::MotifMissed(Fork));
    assert_eq!(MetricId::MotifMissRate(BackRankMate).to_string(), "motif_miss_rate:backRankMate");
    assert_eq!(MateIn4Plus.contract_tag(), "mate_in_4_plus");
}
```

- [ ] **Step 4: Run** `LANE=r2-0 $RUST cargo test -p profile-core --test contracts`. Expected: FAIL (missing items). Implement the types, constants, bands and skeletons. Rerun. Expected: PASS.
- [ ] **Step 5: Compose and env.** `rust-dev` gains `DATA_DIR: /work/data`, `SYZYGY_PATH: /work/data/syzygy`, `MAIA_URL: ${MAIA_URL:-}`. Do not touch other services.
- [ ] **Step 6: Run the gate** `LANE=r2-0 $RUST sh -c "cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo deny check && cargo test --workspace"`. Expected: PASS, `Cargo.lock` updated.
- [ ] **Step 7: Write ADR-R2-1** (the seven crates and why each boundary, the crate table with licences and the 7-day rule, the migration range and the single-migration rule, `DATA_DIR`, the ADR placeholder scheme) and add README rows `ADR-R2-1` … `ADR-R2-18` with the titles used in this plan. Commit `chore(backend): bootstrap r2 crates, dependencies and profile types`.

---

### Task 2: `coaching-input` contract crate with schema parity

**Files:**
- Create: `backend/crates/coaching-input/src/{types.rs,limits.rs,validate.rs,schema.rs}`, `backend/crates/coaching-input/schema/coaching-input-v1.schema.json` (generated, committed)
- Create: `backend/fixtures/coaching-input/{ts-schema.json,SOURCE,valid-sample.json}`
- Test: `backend/crates/coaching-input/tests/{serde.rs,validate.rs,parity.rs}`
- Create: `docs/decisions/ADR-R2-2-coaching-input-contract.md`

**Interfaces:**
- Consumes: `enums.rs` (Task 1), `chess_core::board::Position::from_fen` (R1 T3).
- Produces:
  - `pub const INPUT_VERSION: &str = "coaching-input-v1";` and `pub struct Limits { pv_plies: 16, tags: 12, tag_chars: 40, metrics: 80, weaknesses: 24, strengths: 12, openings: 30, changes: 20, alternatives: 5, candidate_causes: 6, moves_san: 40, san_chars: 12, label_chars: 120, description_chars: 600, id_chars: 64, exemplars_per_item: 5, examples_per_opening: 3, examples_per_change: 6 }` as `pub const LIMITS: Limits`.
  - Structs mirroring the zod objects field for field: `CoachingInput { input_version, account_id: Uuid, platform: Platform, time_class: TimeClass, time_control: Option<TimeControl { base_seconds: u32, increment_seconds: u32 }>, band: BandId, rating: Option<u16>, generated_at: DateTime<Utc>, basis: Basis, area_scores: Vec<AreaScore>, metrics: Vec<Metric>, weaknesses: Vec<RankedWeakness>, strengths: Vec<Strength>, openings: Vec<OpeningVerdict>, changes: Vec<TimelineChange> }`, `Score` (untagged `{cp: i32}` | `{mate: i32}`), `MoveRef { san, uci }`, `ExemplarPosition { game_id, ply, played_at, fen, played, best: Option<MoveRef>, best_pv, refutation_pv, tags, eval_before, eval_after, win_before, win_after }`, `ChangeExample { #[serde(flatten)] position: ExemplarPosition, side: ExampleSide }`, `MetricUnit { Rate, PerGame, Pct, Seconds, Elo }` with `max() -> f64` (1, 1000, 100, 100 000, 4000), `Metric`, `RankedWeakness`, `Strength { area, label, metric_key: Option<String>, exemplars }`, `OpeningVerdict` (`side`, `movesSan`, `performanceVsExpected`, `avgEvalAtOpeningEnd: Option<Score>`, `deviationPly`, `deviationMoveSan`, `alternatives: Vec<Alternative { name, moves_san, band_score, games }>`, `examples`), `AreaScore`, `CandidateCause { kind: CauseKind, description, before: CauseValue, after: CauseValue }` (`CauseValue` untagged `Text(String) | Number(f64)`), `TimelineChange`, `Basis` (`window_start`/`window_end: NaiveDate`). All `#[serde(rename_all = "camelCase", deny_unknown_fields)]`; `Option` fields serialise as `null`.
  - `impl CoachingInput { pub fn validate(&self) -> Result<(), Vec<Issue>>; pub fn to_json(&self) -> serde_json::Value }`, `Issue { path: String, message: String }` (path joined with `.` as `parseCoachingInput` does).
  - `pub fn json_schema() -> serde_json::Value` (schemars, draft 2020-12).

- [ ] **Step 1: Export the TS schema once.** In the m3 worktree, run inside Node 22 (`docker run --rm -v E:/code/chess-improver/.worktrees/m3:/w -w /w node:22-slim sh -c "corepack enable && pnpm install --frozen-lockfile && pnpm --filter @ci/ai exec tsx -e \"import {z} from 'zod'; import {CoachingInput} from './src/input/schema.ts'; console.log(JSON.stringify(z.toJSONSchema(CoachingInput, {target: 'draft-2020-12', unrepresentable: 'any'}), null, 2))\""`) and save stdout as `backend/fixtures/coaching-input/ts-schema.json`. Write `SOURCE` with the worktree commit sha, the command and the date. Refinements (`superRefine`, `refine`) are not exported; `validate()` covers them.
- [ ] **Step 2: Write the failing tests:**
  - `serde.rs`: `score_shapes` (`Score::Cp(-30)` ↔ `{"cp":-30}`, `{"mate":2}`); `option_fields_serialise_as_null` (`best: None` appears as `"best":null`); `unknown_key_is_rejected` (`valid-sample.json` plus `"focus":[]` fails to deserialise); `round_trip_sample` (`valid-sample.json` deserialises and re-serialises to an equal `Value`).
  - `validate.rs`: `valid_sample_passes`; and one failing case each, asserting the issue path: `fen_side_to_move_disagrees_with_ply` (path `weaknesses.0.exemplars.0.fen`), `invalid_fen`, `metric_low_above_high`, `validate_rejects_value_outside_unit_range` (`unit: rate`, `value: 1.2` → path `metrics.0.value`), `posterior_outside_interval`, `events_above_opportunities`, `duplicate_rank` (path `weaknesses.1.rank`), `area_missing_or_duplicated`, `onset_order`, `analysed_above_imported`, `window_start_after_end`, `limits_exceeded` (25 weaknesses → issue at `weaknesses`), `uci_pattern` (`"e2e9"`), `string_too_long` (label of 121 chars).
  - `parity.rs`:
    - `sample_validates_against_ts_schema`: `jsonschema` with format validation on, against `ts-schema.json`, for `valid-sample.json` and for `CoachingInput::to_json()` of a struct built in the test with every `Option` both `Some` and `None` (two inputs);
    - `ts_schema_rejects_what_rust_rejects`: the unknown-key sample is invalid under the TS schema;
    - `schemars_schema_matches_ts_schema_shape`: walk both schemas from the root by property path (resolving `$ref`s, ignoring `$defs` names) and assert, for every object: identical property-name sets, identical `required` sets (all properties), `additionalProperties: false`; for every enum the same value set; for every bound present in the TS schema (`maxLength`, `minLength`, `maxItems`, `minimum`, `maximum`, `pattern`) the same value in the schemars schema;
    - `committed_schema_is_current`: `json_schema()` equals `schema/coaching-input-v1.schema.json` (regenerate with `COACHING_SCHEMA_WRITE=1`).
- [ ] **Step 3: Run** `LANE=r2-k $RUST cargo test -p coaching-input`. Expected: FAIL.
- [ ] **Step 4: Implement.** schemars: annotate bounds with `#[schemars(length(...), range(...), regex(...))]`; make every property required with one schema transform applied to all structs; `Option<T>` renders as `T | null`. `validate()` walks the struct and pushes every issue (it does not stop at the first). FEN check: `Position::from_fen(fen).is_ok()` and side-to-move parity (odd ply = `w`).
- [ ] **Step 5: Run** the tests. Expected: PASS. Generate and commit `schema/coaching-input-v1.schema.json`.
- [ ] **Step 6: Write ADR-R2-2** (the crate is the contract shared by R2 and R3; field-for-field mapping; the committed TS export and how to refresh it; refinements in `validate()`; Rulings R-1…R-8). Commit `feat(coaching-input): m3 contract types with schemars schema and ts parity`.

### Task 3: Profile engine schema, row helpers and stage metrics

**Files:**
- Create: `db/migrations/20261121010000_profile_engine.sql`, `backend/crates/db/src/{profile_rows.rs,stage_metrics.rs}`
- Modify: `backend/crates/db/src/games.rs` (enum variant and columns), `backend/crates/db/src/lib.rs` (two `pub mod` lines)
- Test: `backend/crates/db/tests/profile_schema.rs`

**Interfaces:**
- Consumes: R1 T12 `games`, `MIGRATOR`.
- Produces:
  - `games::AnalysisStatus::Imported` (`"imported"`); `GameRow` gains `ply_count: Option<i32>`, `analysis_tier: Option<i16>`, `analysis_priority: Option<i16>`, `selected_at`, `analysed_at: Option<DateTime<Utc>>`, `enrichment_protocol: Option<String>`; `NewGame` gains `status: AnalysisStatus` (default `Pending`, so R1 callers are unchanged) and `ply_count: Option<i32>`.
  - `stage_metrics::{StageMetrics { job_id: Option<i64>, kind: String, game_id: Option<Uuid>, account_id: Option<Uuid>, total_ms: f64, stages: BTreeMap<String, f64> }, record_stage_metrics(pool, &StageMetrics) -> Result<(), sqlx::Error>}`.
  - `profile_rows`: `insert_game_metrics`, `replace_series_points(conn, game_id, rows)`, `SeriesPointRow`, `upsert_weakness_estimate`, `append_state_history`, `append_kpi_snapshot`, `KpiSnapshotRow` (DB), each a plain runtime-checked statement used by Tasks 31–33.
- [ ] **Step 1: Write the migration.** Exactly TS M2 Task 2's schema, translated to SQL (snake_case), plus R2 additions:
  - `ALTER TYPE analysis_status ADD VALUE 'imported';` (not used as a default in this file);
  - enums `band`, `weakness_state`, `state_row_kind`, `kpi_window`, `reference_kind`, `build_status` (`building`, `active`, `superseded`, `failed`, `unverified`), `opening_verdict`, `line_kind`;
  - `games` columns above; `linked_accounts.analysis_scheduled_at timestamptz`;
  - tables `move_features`, `position_multipv`, `position_threats`, `position_tb`, `game_metrics`, `weakness_series_point`, `weakness_estimates` (plus `k_unweighted real`), `weakness_state_history`, `kpi_snapshot` (append-only, no unique key), `reference_builds` (plus `source_url`, `games_scanned bigint`), `opening_explorer`, `reference_games`, `reference_observations`, `band_reference`, `game_openings`, `repertoire_nodes`, `opening_lines`, with TS M2's keys, FKs (`ON DELETE CASCADE` to games) and indexes;
  - R2: `game_previews (game_id uuid PRIMARY KEY REFERENCES games ON DELETE CASCADE, protocol text NOT NULL, white_accuracy real, black_accuracy real, moves jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT now())`;
  - R2: `ALTER TABLE job_metrics ADD COLUMN stages jsonb;`
  - R2 indexes: `games_account_started_idx ON games (account_id, analysis_started_at)` (fair `served` count), `games_account_status_tc_idx ON games (account_id, analysis_status, time_class)`, `opening_explorer_lookup_idx ON opening_explorer (build_id, band, time_class, fen_key)`.
- [ ] **Step 2: Write the failing tests** (`#[sqlx::test]`): `imported_status_round_trips` (upsert with `status: Imported`, read back); `one_row_per_new_table` (insert into each new table); `kpi_snapshot_keeps_identical_rows` (two equal rows, count 2); `cascade_from_games` (deleting a game removes its `move_features`, `game_metrics`, `weakness_series_point`, `game_openings`, `game_previews`); `record_stage_metrics_writes_stages_json` (`stages->>'multipv_ms'` reads back `12.5`); `r1_callers_default_to_pending` (an R1-style `NewGame` without explicit status gives `pending`).
- [ ] **Step 3: Run** `LANE=r2-k $RUST cargo test -p db --test profile_schema`. Expected: FAIL. Implement. Rerun the whole `-p db` suite. Expected: PASS (R1 tests unchanged).
- [ ] **Step 4:** Run `scripts/db/generate-drizzle-schema.sh` and commit the regenerated files. Commit `feat(db): profile engine schema, imported status and stage metrics`.

---

### Task 4 (P1): Board utilities, material swings and the null move

**Files:**
- Modify: `backend/crates/chess-core/src/board.rs` (append an `impl Position` block only)
- Create: `backend/crates/profile-core/src/board/{mod.rs,material.rs,line.rs,null_move.rs,attacks.rs}`, `backend/crates/profile-core/tests/common/positions.rs` (fixtures shared by Tasks 4–6)
- Test: `backend/crates/profile-core/tests/board.rs`

**Interfaces:**
- Consumes: R1 T3 `Position`.
- Produces:
  - in chess-core: `Position::play_uci(&mut self, uci: &str) -> Result<PlayedMove, IllegalMove>` (king-destination castling UCI, as T3), `Position::board(&self) -> &cozy_chess::Board`, `Position::is_check(&self) -> bool`;
  - `PIECE_VALUE` (p 1, n 3, b 3, r 5, q 9, k 0), `MATE_GAIN = 100.0`;
  - `material_balance(fen: &str, pov: Color) -> i32`, `piece_count(fen: &str) -> u32` (kings included), `material_signature(fen: &str) -> String` (stronger side first, `"KRPvKR"`);
  - `LineStep { uci, san, fen_before, fen_after, mover: Color, captured: Option<char>, is_check, is_mate, promotion: Option<char>, from: String, to: String }`;
  - `play_line(fen: &str, pv: &[String], max_plies: Option<usize>) -> Vec<LineStep>` (stops at the first illegal move, never panics);
  - `settled_swing(fen: &str, pv: &[String], pov: Color, max_plies: usize /* 8 */, settle_extra: usize /* 4 */) -> Swing`;
  - `null_move_fen(fen: &str) -> Option<String>`;
  - `pinned_to(fen, square) -> Option<(String /* pinner */, String /* behind */)>`, `is_en_prise(fen, square) -> bool`, `attackers_of(fen, square, color) -> Vec<String>`, `defenders_of(fen, square, color) -> Vec<String>`.
- Algorithm (`settled_swing`), as TS M2: `B0 = material_balance(fen, pov)`. For each `pov` ply k ≤ `max_plies`, `settled(k)` = balance after ply k extended through the recapture chain (following PV moves that capture on the previous move's destination square, at most `settle_extra` plies) minus B0. `depth` = first k with `settled(k) ≥ 2` or ply k delivering mate. `gain` = max `settled(k)`, or `MATE_GAIN` if the line mates. `mate_in = (mating_ply_index + 1) / 2` when the PV ends in mate by pov.

- [ ] **Step 1: Write the failing tests** with TS M2 Task 3's fixture table, verbatim (all White to move, pov white): hanging `{9, 1, None}`, fork `{5, 3, None}`, pin `{2, 3, None}`, skewer `{9, 3, None}`, discovered `{9, 3, None}`, deflection `{4, 3, None}`, promotion `{8, 1, None}`, backRank `{100, 1, Some(1)}`, mateIn2 `{100, 3, Some(2)}`, smothered `{100, 1, Some(1)}`, quiet `{0, None, None}` (FENs and PVs exactly as in TS M2; put them in `tests/common/positions.rs` as `pub const FIXTURES: &[(&str, &str, &[&str])]`). Plus:
  - `play_line(START, ["e2e4","e2e4"]).len() == 1`;
  - `material_signature("8/4k3/8/4K3/4P3/8/8/8 w - - 0 1") == "KPvK"`, `piece_count` of it is 3;
  - `null_move_fen("4k3/8/8/r7/8/8/8/R3K3 b - - 0 1") == Some("4k3/8/8/r7/8/8/8/R3K3 w - - 0 1")`;
  - `null_move_fen_returns_none_in_check`: `"4k3/8/8/8/8/8/8/R3K2r w - - 0 1"` → `None`;
  - `null_move_clears_en_passant`: `"rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3"` → `"… b KQkq - 0 3"`;
  - `pinned_to` on the pin fixture after `f2f4`, square `e5` → `Some(("e1", "e8"))`;
  - `play_uci_castling`: `play_uci("e1g1")` after `1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5` gives san `O-O`.
- [ ] **Step 2: Run** `LANE=r2-p1 $RUST cargo test -p profile-core --test board`. Expected: FAIL.
- [ ] **Step 3: Implement** over cozy-chess (attacks via `cozy_chess::get_*_moves` and the board's piece sets) and `Position` (SAN and FEN). `null_move_fen` flips side, clears en passant, keeps clocks, returns `None` when the side to move is in check or `Position::from_fen` rejects the result.
- [ ] **Step 4: Run** the tests (and `$RUST cargo test -p chess-core`, to prove the append changed nothing). Expected: PASS. Commit `feat(profile-core): material swings, line replay and null-move fen`.

### Task 5 (P1): Motif tagger (clean-room)

**Files:**
- Create: `backend/crates/profile-core/src/motifs/{mod.rs,tag_line.rs,rules.rs}`, `docs/decisions/ADR-R2-3-motif-tagger-clean-room.md`
- Test: `backend/crates/profile-core/tests/motifs.rs`

**Interfaces:**
- Consumes: Task 4.
- Produces: `tag_line(fen: &str, pv: &[String], pov: Color, max_plies: usize /* 8 */) -> LineAnalysis`. Tags only when `swing.depth.is_some()`, only from plies ≤ depth + 1, in `MotifTag` order without duplicates. The eleven rules are exactly TS M2 Task 4's (hangingPiece, fork, pin, skewer, discoveredAttack, deflection, sacrifice, promotion, mate/mateIn1–3/mateIn4Plus, matingNet = mateIn ≥ 2, backRankMate, smotheredMate), one function each in `rules.rs`.

- [ ] **Step 1: Write the failing tests**, TS M2 Task 4's assertions translated: hanging → exactly `[HangingPiece]`; fork contains Fork, not HangingPiece; pin contains Pin not Skewer; skewer contains Skewer not Pin; discovered contains DiscoveredAttack; deflection contains Deflection and Sacrifice; promotion contains Promotion; backRank contains Mate, MateIn1, BackRankMate; mateIn2 contains Mate, MateIn2, MatingNet, BackRankMate, Sacrifice and not Deflection; smothered contains MateIn1, SmotheredMate, not BackRankMate; quiet → `[]`; fork with pov Black → `[]` (no swing, no tags).
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test motifs`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-3** (own rules from the published description in improvement-engine §4.4, nothing from `cook.py` (AGPL), the tag list, "no swing, no tags", camelCase keys vs snake_case contract tags). Commit `feat(profile-core): clean-room motif tagger`.

### Task 6 (P1): Per-move features and enriched-move assembly

**Files:**
- Create: `backend/crates/profile-core/src/features/{mod.rs,derive.rs,assemble.rs}`
- Test: `backend/crates/profile-core/tests/features.rs`

**Interfaces:**
- Consumes: Tasks 4–5; R1 `win_percent`, `flip_score`, `MoveRecord`, `PositionEval`, `Terminal`.
- Produces:
  - `derive_move_features(color: Color, fen_before: &str, fen_after: &str, uci: &str, raw: Option<&RawMoveFeatures>) -> MoveFeatures`;
  - `assemble_enriched_moves(records: &[MoveRecord], evals: &[PositionEval], raw: &[Option<RawMoveFeatures>], terminal: Option<Terminal>) -> Vec<EnrichedMove>` (`raw[i]` belongs to `records[i]`; scores to mover POV; a final checkmate ply gets `score_after = Mate(1)`; `fen_after` is the next record's `fen_before`, or the last eval position's FEN replayed for the final ply).
- Rules exactly TS M2 Task 5: `multipv_gap = W(lines[0]) − W(lines[1])` (≥ 2 lines), `is_critical = gap ≥ 15`; `playable` = lines within 5 W% of the best; `in_top2`; `best`/`refutation`/`threat` via `tag_line`; `threat_existed = threat.gain ≥ 2 ∧ depth ≤ 4`; `threat_addressed` with the first-capture-square rule. No MultiPV → `is_critical`, `multipv_gap`, `playable`, `in_top2` are `None`.

- [ ] **Step 1: Write the failing tests** (TS M2 Task 5 values): the threat fixture `"4k3/8/8/r7/8/8/8/R3K3 b - - 0 1"` with `threat = {null_fen: null_move_fen(fen), pv: ["a1a5"], score: Cp(500)}`: played `e8e7` with `refutation_pv ["a1a5"]` → `threat_existed Some(true)`, `threat_addressed Some(false)`; played `a5a1` with `["e1d2"]` → `Some(true)`, `Some(true)`. MultiPV mover cp `[300, 20, 10]` → critical, playable 1; `[30, 25, 20]` → not critical, playable 3; played = `lines[1].uci` → `in_top2 Some(true)`. `raw: None` → `is_critical None, best None, threat_existed None`. Assembly on R1's `scholars-mate.pgn` (`build_move_records`, raw all `None`): `moves[6].score_after == Mate(1)`; `moves[1].score_before == flip_score(evals[1].score)`; `moves[0].fen_after == moves[1].rec.fen_before`.
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test features`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(profile-core): per-move tactical features and enriched moves`.

### Task 7 (P1): Episodes, time trouble, endgame type and sessions

**Files:**
- Create: `backend/crates/profile-core/src/weakness/{episodes.rs,time_trouble.rs,endgame.rs,sessions.rs}`
- Test: `backend/crates/profile-core/tests/episodes.rs`

**Interfaces:**
- Consumes: Task 4 `material_balance`; R1 `MoveRecord`, `Color`.
- Produces (rules exactly TS M2 Task 6):
  - `Episode { kind: EpisodeKind /* Winning|Losing */, start_ply, end_ply, outcome: Outcome /* Won|Drawn|Lost */, drop_ply: Option<u32> }`; `EpisodeOpts { t_win: 85.0, t_lose: 15.0, min_moves: 3, min_ply: 21 }` (`Default`); `extract_episodes(moves: &[MoveRecord], player: Color, score: Option<f64>, o: &EpisodeOpts) -> (Option<Episode>, Option<Episode>)` (drop ply = first own ply with `win_after < 65`);
  - `detect_time_trouble(moves, player, base_seconds: Option<u32>) -> TimeTrouble { in_trouble: Option<bool>, first_ply: Option<u32> }` (clock < 10% of base and (move number < 40 or ≥ 10 own moves remain));
  - `endgame_type(fen) -> EndgameType`; `BASIC_SIGNATURES` = `KPvK, KQvK, KRvK, KRvKP, KRPvKR, KQvKP, KPvKP`;
  - `expected_score(own: f64, opp: f64) -> f64 = 1 / (1 + 10^((opp − own)/400))`;
  - `SessionGame { game_id, played_at, est_duration_seconds: f64, score: Option<f64>, expected_score: Option<f64> }`, `build_sessions(games: &[SessionGame], gap_minutes: i64 /* 15 */) -> Vec<SessionInfo>`;
  - `is_time_loss(ctx: &GameContext) -> bool` (Lichess `outoftime`/`timeout`, chess.com `/on time/i`, player not winning), `is_resignation(ctx) -> bool`.

- [ ] **Step 1: Write the failing tests** (TS M2 Task 6 values, all of them): winning episode `[90, 88, 91, 70]` at plies 21–27, ply 27 win_after 60, score 0.5 → `{start_ply 21, Drawn, drop_ply Some(27)}`; same from ply 15 → `None`; losing `[10, 12, 9]`, score 0.5 → `Drawn`; base 300 ply 61 clock 25 → `{Some(true), Some(61)}`; clock 31 → `Some(false)`; clock 25 at ply 89 with 3 own moves left → `Some(false)`; no clocks → `in_trouble None`; `endgame_type("8/5k2/8/8/3R4/8/2r5/4K3 w - - 0 1") == Rook`; games at 10:00, 10:04, 11:00 (180 s each) → session ids `[1,1,2]`, indexes `[1,2,1]`; `expected_score(1500., 1500.) == 0.5`; chess.com termination `"Hikaru won on time"`, player black, result `1-0` → `is_time_loss`.
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test episodes`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(profile-core): episodes, time trouble, endgame types and sessions`.

### Task 8 (P1): Weakness detectors W1–W12

**Files:**
- Create: `backend/crates/profile-core/src/weakness/{detectors.rs,observe_game.rs,tilt.rs}`, `backend/crates/profile-core/tests/common/make_game.rs`, `docs/decisions/ADR-R2-4-weakness-metric-rulings.md`
- Test: `backend/crates/profile-core/tests/detectors.rs`

**Interfaces:**
- Consumes: Tasks 1, 6, 7 (W9d uses a local rank-correlation helper, so no dependency on Lane P3).
- Produces: `observe_game(game: &AnalysedGame, t_win: f64 /* 85 */) -> GameObservation`; `tilt_residual_difference(sessions: &[SessionInfo]) -> (Vec<f64> /* tilted */, Vec<f64> /* other */)` ("tilted" = `index_in_session ≥ 4` or `after_loss`). The opportunity/event table, EPL attribution by `EPL_PRECEDENCE`, `exemplar_plies` (event plies by ΔW desc), and the series rules (`motif.missed:*`, `motif.allowed:*`, `endgame:*`, `opening:*`) are exactly TS M2 Task 7.
- `tests/common/make_game.rs`: `make_game() -> AnalysedGame` (30 eligible player moves with ΔW 0, neutral features, blitz 300+0 with clocks) plus builder helpers `with_move(i, |m| ...)`.

- [ ] **Step 1: Write the failing tests**, every TS M2 Task 7 case: W1 `{events 1, opportunities 30, epl 0.2}`; W2 takes the EPL (`W2.epl == 0.2`, `W1.epl == 0`, `W1.events == 1`); decided and opponent moves not counted; W3.shallow / W3.deep / `in_top2` → 0 events; W4.missed; W4.allowed; W8.basics on `"8/8/4k3/8/4P3/8/4K3/7r w - - 0 1"`; W9b event and opportunity-only; `w9d_rho == Some(1.0)`; W10 event and the `is_critical` exclusion; W11 at pOk 0.9 and the 0.7 exclusion; W5 `{1, 1, 0.5}`; W7.traps at ply 15; the `motif.allowed:fork` series `{events 1, opportunities 30}`; `no_clocks_w9_keys_absent` (W9a/W9b/W9c absent from the map, `w9d_rho None`).
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test detectors`. Expected: FAIL. Implement one function per key in `detectors.rs`, composed in `observe_game`. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-4** with TS M2 Task 7's rulings (sub-series split; W10, W9b, W9c as binomial counts; W9d and W12 continuous and not ranked; depth 4 counts as deep; W6 event cost 0.5; W1's settled swing as the SEE stand-in; tablebase-eligible ≤ 5 men; fixed T_win 85). Commit `feat(profile-core): weakness detectors W1-W12`.

### Task 9 (P1): Per-game metrics

**Files:**
- Create: `backend/crates/profile-core/src/weakness/game_metrics.rs`
- Test: `backend/crates/profile-core/tests/game_metrics.rs`

**Interfaces:**
- Consumes: Tasks 6–8.
- Produces: `compute_game_metrics(game: &AnalysedGame, obs: &GameObservation, strengths: Vec<StrengthPly>) -> PlayerGameMetrics`, with TS M2 Task 8's definitions (oneMoveBlunders, missedPunish (miss = ΔW ≥ 10), sharp, `eval_trend_15_to_35` (W after ply 70 − W after ply 30), `critical_time_ratio`, `win_at_opening_end`, `resigned_saveable` (final W ≥ 25), `expected_score`), plus `opening.eval_at_opening_end_white_cp` (R-8).

- [ ] **Step 1: Write the failing tests** (TS M2 Task 8): judgements `[blunder, mistake, inaccuracy, None…]` → 1/1/1; accuracies opening `[100, 80]`, middlegame `[60]` → 90 / 60 / `None`; opponent blunder at ply 10 then ΔW 12 at ply 11 → `missed_punish {1, 1}`; ratings 1500 vs 1700 → `expected_score ≈ 0.2403` (4 dp); Lichess `outoftime` with the player losing → `lost_on_time`; `weakness[W1] == obs.binomial[W1]`; strengths passed through; `json_shape_is_camel_case_version_1` (`serde_json::to_value(m)["version"] == 1`, key `accuracyByPhase` present).
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test game_metrics`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(profile-core): per-game metrics`.

### Task 10 (P1): Strength exemplar extraction

**Files:**
- Create: `backend/crates/profile-core/src/weakness/strengths.rs`
- Test: `backend/crates/profile-core/tests/strengths.rs`

**Interfaces:**
- Consumes: Tasks 6–7.
- Produces: `extract_strength_plies(game: &AnalysedGame) -> Vec<StrengthPly>`. Rules exactly TS M2 Task 9 (only-move found, tactic found via `best_pv[0]`, conversion and defence at the first own critical ply inside the episode or its start; at most 3 per kind, by `multipv_gap` desc then ply; eligible player moves only).

- [ ] **Step 1: Write the failing tests** (TS M2 Task 9's six cases): only-move; tactic with best.gain 3; won winning episode without critical plies → conversion at `start_ply`; drawn losing episode → defence; four critical only-moves → 3; decided positions → none.
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test strengths`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(profile-core): strength exemplar extraction`.

---

### Task 11 (P3): Statistics numerics

**Files:**
- Create: `backend/crates/profile-core/src/stats/{rng.rs,special.rs,beta.rs,intervals.rs,quantiles.rs,beta_binomial.rs,correlation.rs,weights.rs}`
- Test: `backend/crates/profile-core/tests/stats.rs`

**Interfaces:**
- Produces: `Rng::new(seed: u32)` with `next_f64(&mut self) -> f64` (mulberry32, bit-identical to the TS reference algorithm); `lgamma(x)`; `reg_inc_beta(x, a, b)` (continued fraction); `beta_quantile(p, a, b)` (bisection to 1e-10); `sample_beta(rng, a, b)` (Marsaglia–Tsang); `Z80`; `Interval { low, high }`; `wilson(k: f64, n: f64, z: f64) -> Interval` (`n = 0` → `{0, 1}`); `bootstrap_mean_interval(values: &[f64], resamples: usize /* 1000 */, seed: u32 /* 1 */, level: f64 /* 0.8 */) -> Option<Interval>` (`None` when n < 2); `quantile(sorted: &[f64], p: f64) -> f64` (type 7); `QuartileRef { p25, p40, median, p75 }` and `percentile_from_quartiles(value, r: &QuartileRef, higher_is_better: bool) -> f64` (piecewise linear as TS M2, clamped [1, 99], lower-is-better → `100 − raw`); `fit_beta_binomial(data: &[(f64, f64)]) -> BetaBinomialFit { mu, phi }` (60 log-spaced φ in [1, 1e5]); `spearman(x, y) -> Option<f64>`; `weighted_counts(points: &[(f64 /* events */, f64 /* opps */, f64 /* age_days */)], half_life_days: f64 /* 60 */) -> (f64, f64)`.

- [ ] **Step 1: Write the failing tests**, TS M2 Task 15's assertions verbatim: `beta_quantile(0.1, 16, 684) ≈ 0.0159747` (6 dp), `(0.9, …) ≈ 0.0303238`, `1 − reg_inc_beta(0.02, 16, 684) ≈ 0.672363` (5 dp); `wilson(7, 20) ≈ [0.229497, 0.493269]`; `wilson(0, 10) = {0, ≈0.141069}`; `wilson(0, 0) = {0, 1}`; `wilson(2, 10).high ≈ 0.398404`; mean of 100 000 `sample_beta(Rng::new(42), 2, 5)` within 0.005 of 2/7; two `Rng::new(1)` give the same first 5 draws; `quantile([1,2,3,4], 0.5) == 2.5`, `(…, 0.25) == 1.75`; percentiles of `[0.02, 0.015, 0.03, 0, 0.04]` against `{0.01, 0.015, 0.02, 0.03}` lower-better → `[50, 60, 25, 99, 1]`; `fit_beta_binomial(20 × (5, 100)) == {0.05, 1e5}`; mixed 10 × (2,100) + 10 × (8,100) → `mu ≈ 0.05` (10 dp), `phi < 200`; `spearman([1,2,3],[3,2,1]) == Some(-1)`, `([1,1,2],[1,2,3]) ≈ 0.866025`, constant → `None`; `weighted_counts([(1,10,0),(2,10,60)]) == (2, 15)`; `bootstrap_mean_interval([5]) == None`.
- [ ] **Step 2: Run** `LANE=r2-p3 $RUST cargo test -p profile-core --test stats`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(profile-core): seeded statistics numerics`.

### Task 12 (P3): Posteriors, P_imp and priority

**Files:**
- Create: `backend/crates/profile-core/src/stats/{posterior.rs,priority.rs}`, `docs/decisions/ADR-R2-5-posterior-priority-conventions.md`
- Test: `backend/crates/profile-core/tests/posterior.rs`

**Interfaces:**
- Consumes: Task 11; Task 1 constants.
- Produces: `BetaPosterior { alpha, beta, mean, interval }`; `posterior(k, n, mu, n0) -> BetaPosterior`; `prob_above(p, threshold) -> f64`; `p_improvement(current, baseline, delta_rel: f64 /* 0.15 */, draws: usize /* 10_000 */, seed: u32 /* 1 */) -> f64`; `p_regression(current, baseline, factor /* 1.15 */, draws, seed) -> f64`; `EstimateReason { Ok, NoBenchmark, InsufficientData }`; `WeaknessEstimate { key, k, n, n_unweighted, k_unweighted, posterior, band_median: Option<f64>, band_p40: Option<f64>, percentile: Option<f64>, confidence, excess, trainability, priority, precise, min_evidence_met, reason }`; `EstimateInput<'a> { key, points: &'a [(f64, f64, f64, f64) /* events, opps, epl, age_days */], games_in_window: u32, band, reference: Option<&'a BandReferenceStat>, fatigue: f64 }`; `estimate_weakness(i: &EstimateInput) -> WeaknessEstimate`; `rank_weaknesses(v: Vec<WeaknessEstimate>) -> Vec<WeaknessEstimate>`. Rules exactly TS M2 Task 16 (183-day window, half-life 60, prior `{pooled_rate, N0}`, `Beta(1,1)` without reference, confidence, excess, priority, precise, min evidence, `insufficient_data` under 30 games).

- [ ] **Step 1: Write the failing tests** (TS M2 Task 16): the W1 case (one point `(10, 400, 2.5, 0)`, 40 games, B1, ref `{pooled 0.02, median 0.02, p40 0.017, epl_per_event 0.25}`) → `alpha 16, beta 684`, mean ≈ 0.0228571, interval ≈ [0.0159747, 0.0303238], confidence ≈ 0.672363, excess ≈ 0.0071429 (6 dp), priority ≈ 0.0048026, `precise false`, `min_evidence_met false`, `Ok`, `k_unweighted 10`; `no_reference_gives_priority_0` (`NoBenchmark`, 0); 29 games → `InsufficientData`, 0; a point aged 200 days ignored; `p_improvement(Beta(12,988), Beta(30,970))` within 0.01 of 0.99096, both `Beta(50,950)` within 0.01 of 0.20296, `p_regression(Beta(45,955), Beta(30,970))` within 0.01 of 0.87883, identical with the same seed; `rank_weaknesses` drops W12 and W9d and sorts by priority desc then key.
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test posterior`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-5** (80% everywhere, 6-month window with 60-day half-life, excess definition, fatigue 0 until M6/M7, no ranking without a benchmark or 30 games, mulberry32 as the seeded generator). Commit `feat(profile-core): weakness posteriors, p_imp and priority`.

### Task 13 (P3): Change detectors (BOCPD, CUSUM, EWMA)

**Files:**
- Create: `backend/crates/profile-core/src/timeline/{bocpd.rs,cusum.rs,ewma.rs}`
- Test: `backend/crates/profile-core/tests/timeline_detectors.rs`

**Interfaces:**
- Consumes: Task 11 `lgamma`.
- Produces: `BocpdOpts { hazard: f64 /* 1/200 */, prior_a: f64, prior_b: f64, max_run: usize /* 500 */ }`; `run_bocpd(obs: &[(u32, u32)], o: &BocpdOpts) -> Vec<Vec<f64>>` (run-length posterior after each observation); `ChangeSummary { p_change, map_run, onset_index, onset_low, onset_high }`; `change_summary(post: &[f64], t: usize, window_games: usize /* 50 */, min_prior_games: usize /* 10 */, level: f64 /* 0.8 */) -> ChangeSummary`; `cusum(obs, p0, p1, h /* 4 */) -> (Option<usize> /* 1-based alarm */, f64)`; `ewma_drift(x: &[f64], lambda, level, sigma, k) -> (Option<usize>, f64)`.
- Algorithm: Adams–MacKay in log space with the Beta-binomial predictive, exactly the TS M2 Task 17 pseudocode (grow / change-point / truncate to `max_run` / normalise); `p_change = Σ_{r < min(window, t − min_prior)} post[r]`, `onset_index = t − map_run + 1`; bounds from the smallest highest-mass run-length set covering `level`.

- [ ] **Step 1: Write the failing tests** with prior `(1.5, 48.5)`, hazard 1/200, max_run 500 (TS M2 Task 17): up (60 × (1,50) then 40 × (3,50)) → at t = 100 `p_change ≥ 0.99`, `onset_index 61`, `onset_low ≤ 61 ≤ onset_high`, width ≤ 6; down → `onset_index 61`; flat alternating → `p_change < 0.2`; 60 × (0,0) → `< 0.3`; `cusum` up `(0.02, 0.03, 4)` → alarm 66; down `(0.06, 0.04)` → 67; flat → `None`; `ewma_drift([0.02, 0.02, 0.06, 0.06], 0.1, 0.02, 0.01, 2.0)` → alarm 4.
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test timeline_detectors`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(profile-core): bocpd, cusum and ewma change detectors`.

### Task 14 (P3): Weakness timeline states, onsets and cause hints

**Files:**
- Create: `backend/crates/profile-core/src/timeline/{states.rs,series.rs,causes.rs}`, `docs/decisions/ADR-R2-6-timeline-detector-and-states.md`
- Test: `backend/crates/profile-core/tests/timeline_states.rs`

**Interfaces:**
- Consumes: Tasks 12, 13.
- Produces: `DETECTOR_VERSION = "tl-v1"`; `Signals { confirmed_up, confirmed_down, cusum_up: bool, confidence_above: f64, at_or_below_median: bool, below_p40_streak: u32, p_imp: Option<f64>, drift: bool }`; `next_state(prev: Option<WeaknessState>, s: &Signals) -> WeaknessState` (the TS M2 Task 18 rule list, first match wins); `SeriesPoint { game_id, played_at, events, opportunities, protocol }`; `RowKind { Transition, Onset }`; `Onset { game_id, played_at, low: DateTime<Utc>, high: DateTime<Utc> }`; `TimelineRow { kind, from: Option<WeaknessState>, to, at_game_id, at_played_at, onset: Option<Onset>, p_change: Option<f64>, rate_before: Option<f64>, rate_after: Option<f64>, evidence: Evidence { k, n, confidence } }`; `SeriesRef { median, p40, pooled_rate }`; `evaluate_series(points: &[SeriesPoint], reference: Option<&SeriesRef>, n0: f64, min_evidence: f64) -> SeriesOutcome { state, rows: Vec<TimelineRow>, p_imp: Option<f64> }` (prior `a = 50·mu, b = 50·(1 − mu)`; confirmation at `p_change ≥ 0.8`, relative change ≥ 0.15, opportunities since onset ≥ `min_evidence`; CUSUM at `p1 = 1.5·p0` after ≥ 10 games, reset per confirmation; EWMA drift from entering `resolved`; no reference → `not_an_issue` only); `CauseGame { tc: String, opponent_rating: Option<i32>, line_key: Option<String>, played_at, protocol }`; `cause_hints(before: &[CauseGame], after: &[CauseGame]) -> Vec<coaching_input::CandidateCause>` (the five kinds and triggers of TS M2; deterministic English descriptions such as `"Most games moved from 300+0 to 180+0"`).

- [ ] **Step 1: Write the failing tests** (TS M2 Task 18): the six `next_state` table cases; `evaluate_series` with ref `{0.02, 0.017, 0.02}`, n0 300, min_evidence 1000: 100 × (1,50) → one row `{from None, to NotAnIssue}`; 60 × (1,50) + 60 × (3,50) → final `Active` with an onset row at `points[60]` ± 2 games; plus 80 alternating (0,50)/(1,50) → `Improving` before `Resolved`, final `Resolved`; plus 30 × (3,50) → `Relapsed` after `Resolved`; `reference None` → only `NotAnIssue`. `cause_hints`: 20 × `300+0` then 15 × `180+0` + 5 × `300+0` → `{TimeControlMix, before "300+0", after "180+0"}`; opponents 1500 → 1600 → `RatingLevel`; a line 0% → 40% → `NewOpening`; equal inputs → `[]`; every description ≤ 600 chars.
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test timeline_states`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-6** (TS M2 Task 18's points: hazard, window, 10 prior games, confirmation rule, CUSUM early warning, onset rows, W9d/W12 not run through BOCPD, deterministic full recompute). Commit `feat(profile-core): weakness timeline states, onsets and cause hints`.

### Task 15 (P3): KPI catalogue, snapshots and area scores

**Files:**
- Create: `backend/crates/profile-core/src/kpi/{catalogue.rs,snapshots.rs,area_scores.rs,iso_week.rs}`, `docs/decisions/ADR-R2-7-kpi-catalogue-and-area-scores.md`
- Test: `backend/crates/profile-core/tests/kpi.rs`

**Interfaces:**
- Consumes: Task 11; Task 1 types; Task 7's `is_time_loss` only through `PlayerGameMetrics.lost_on_time` (no code dependency).
- Produces:
  - `KpiGame<'a> { ctx: &'a GameContext, metrics: &'a PlayerGameMetrics, session: Option<&'a SessionInfo> }`;
  - `KpiKind { Mean, Rate, Last }`; `Extracted { Value(f64), Counts(f64, f64) }`; `KpiDefinition { metric: MetricId, area: Option<Area>, kind, contract_unit: Option<MetricUnit>, higher_is_better: Option<bool>, min_n: u32, label: &'static str, extract: fn(&KpiGame) -> Option<Extracted> }`;
  - `kpi_catalogue() -> &'static [KpiDefinition]`: the 36 base rows of TS M2 Task 19's table (same metric ids, kinds, areas, directions and minimum N), plus `motif_miss_rate:<tag>` (rate, tactics, minN 20) and `weakness:<key>` (rate, `weakness_area(key)`, `min_n = N0`). `contract_unit` per R-2: `rating` elo; `accuracy*`, `quiet_loss_per_move`, `eval_after_opening`, `first_game_accuracy`, `later_games_accuracy` pct; `*_per_game` and `tb_errors_per_endgame` per_game; every rate row, `score_after_loss`, `score_after_win` rate; `time_per_move.*`, `time_in_opening` seconds; `perf_vs_expected`, `eval_trend_15_35`, `critical_time_ratio`, `book_exit_ply`, `session_accuracy_decline` `None`;
  - `iso_week_key(d: DateTime<Utc>) -> String`;
  - `Window { LastN(usize /* 50 */), Week(String) }`; `KpiValue { metric, window_kind, window_key, value: Option<f64>, value_adjusted: Option<f64>, n: u32, ci_low: Option<f64>, ci_high: Option<f64> }`; `compute_kpis(games: &[KpiGame], w: &Window, reference: Option<&BandReference>) -> Vec<KpiValue>` (`window_key` `"last50"` or the ISO week; opponent adjustment by `opp_slope_per_100 × mean((opp − own)/100)`, interval shifted by the same amount; `None` for `Last`);
  - `Comparison { Improved, Worse, NoClearChange }`; `compare_kpi(a: &KpiValue, b: &KpiValue, def: &KpiDefinition) -> Comparison` (intervals separate, or two-proportion |z| ≥ 1.96 for rates);
  - `area_components(area) -> &'static [MetricId]` (TS M2's `AREA_COMPONENTS`); `compute_area_scores(kpis: &[KpiValue], reference: Option<&BandReference>) -> Vec<coaching_input::AreaScore>` (TS M2 rules; mindset has no benchmark: `score None, sufficient false, need_more None`).

- [ ] **Step 1: Write the failing tests** (TS M2 Task 19): `iso_week_key(2026-11-21) == "2026-W47"`, `(2027-01-01) == "2026-W53"`, `(2026-01-01) == "2026-W01"`; blunders over `[0, 2, 1]` → value 1, n 3, interval contains 1; `time_trouble_rate_counts_only_clocked_games` (10 clocked, 2 in trouble, + 5 daily → n 10, `ci_high ≈ 0.398404`); 60 games with `LastN(50)` → n 50, most recent; a week window holds only that week; `opp_slope_per_100 = 2` with mean +100 → `value_adjusted = value − 2`; `compare_kpi` rates 30/100 → 10/100 (lower better) `Improved`, 12/100 vs 10/100 `NoClearChange`; `area_with_too_little_data_reports_have_and_need` (endgame: accuracy.endgame n 12 → `{score None, sufficient false, need_more Some(8)}`); components at the band median → score 50; `catalogue_has_36_base_rows_and_contract_units` (every row with `contract_unit = None` is one of the five listed above).
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test kpi`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-7** (catalogue, windows, opponent adjustment, append only when value or n changed, area method, mindset without a benchmark, R-2 contract units). Commit `feat(profile-core): kpi catalogue, snapshots and area scores`.

---

### Task 16 (P2): Opening classification from the CC0 chess-openings dataset

**Files:**
- Create: `backend/crates/profile-core/data/chess-openings/{a,b,c,d,e}.tsv`, `backend/crates/profile-core/data/chess-openings/SOURCE`, `backend/crates/profile-core/src/openings/{classify.rs,dataset.rs}`
- Test: `backend/crates/profile-core/tests/openings.rs`

**Interfaces:**
- Consumes: R1 T3 `Position::play_san`, R1 T2 `fen_key`.
- Produces: `openings() -> &'static HashMap<String /* fen_key */, OpeningName { eco, name }>` (built once with `OnceLock` from `include_str!` TSVs, replaying each `pgn`; duplicates keep the longest name); `Classified { eco, name, fen_key, ply, mover }`; `classify_opening(plies: &[(String /* fen_after */, Color)]) -> Option<Classified>` (deepest named position); `line_family(name) -> &str`; `line_key(color: Color, name: &str) -> String` (R-7: slug ≤ 40 + hash suffix); `line_kind(entry_mover: Color, player: Color) -> LineKind { Chosen, Faced }`.

- [ ] **Step 1:** Download the five TSVs at the current `lichess-org/chess-openings` master sha (`gh api repos/lichess-org/chess-openings/commits/master -q .sha`) and write `SOURCE` (URL, sha, CC0-1.0).
- [ ] **Step 2: Write the failing tests** (TS M2 Task 12): `1.e4 c6 2.d4 d5 3.e5` → `B12`, name starts `"Caro-Kann Defense: Advance Variation"`; Italian → `C50`, `"Italian Game"`; `1.Nf3 d5 2.d4` and `1.d4 d5 2.Nf3` equal; `line_family("Sicilian Defense: Najdorf Variation, English Attack") == "Sicilian Defense: Najdorf Variation"`; `line_kind(Black, White) == Faced`; start position → `None`; `line_key_fits_64_chars` (a 90-char name gives a key ≤ 48 chars ending in 6 hex digits; `"opening:" + key` ≤ 64); `dataset_loads_under_200ms` (≥ 3 000 entries).
- [ ] **Step 3: Run** `LANE=r2-p2 $RUST cargo test -p profile-core --test openings`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(profile-core): opening classification from lichess chess-openings (CC0)`.

### Task 17 (P2): `refdata` — reference builds, explorer and band reference storage

**Files:**
- Create: `backend/crates/refdata/src/{builds.rs,explorer.rs,band_reference.rs,reference_games.rs}`
- Test: `backend/crates/refdata/tests/{builds.rs,explorer.rs,band_reference.rs}`

**Interfaces:**
- Consumes: Task 3 tables; Task 1 `Band`, `BandReferenceStat`, `MetricId`.
- Produces:
  - `BuildKind { BandReference, OpeningExplorer }`; `BuildStatus { Building, Active, Superseded, Failed, Unverified }`; `NewBuild { kind, source_month, source_url, params: Value, protocol: Option<String>, enrichment_protocol: Option<String>, difficulty_model: Option<String> }`; `create_build(pool, &NewBuild) -> Result<Uuid>`; `set_source_sha(pool, id, sha: &str, games_scanned: i64)`; `activate_build(pool, id) -> Result<()>` (one transaction: this build `active` and `activated_at`, the previous active build of the same kind `superseded` and its rows deleted); `fail_build(pool, id, error: &str)`; `mark_unverified(pool, id)`; `active_build(pool, kind) -> Result<Option<Uuid>>`.
  - `ExplorerKey { fen_key: String, band: Band, time_class: TimeClass, uci: String }`; `ExplorerCounts { white: i64, draws: i64, black: i64 }`; `flush_explorer(pool, build_id, rows: &[(ExplorerKey, ExplorerCounts)]) -> Result<()>` (`INSERT … SELECT * FROM UNNEST(…) ON CONFLICT (build_id, fen_key, band, time_class, uci) DO UPDATE SET white = opening_explorer.white + excluded.white, …`, chunks of 5 000); `prune_explorer(pool, build_id, min_row_count: i64) -> Result<u64>`; `ExplorerMove { uci, white, draws, black, total }`; `query_explorer(pool, fen_key, band, tc) -> Result<Vec<ExplorerMove>>` (active build only, total desc); `load_explorer_index(pool, fen_keys: &[String], band, tc) -> Result<HashMap<String, Vec<ExplorerMove>>>` (batches of 1000); `in_theory(index, fen_key, uci) -> bool` (position total ≥ 50 and share ≥ 5%).
  - `insert_reference_games(pool, build_id, games: &[NewReferenceGame]) -> Result<Vec<Uuid>>`; `write_observations(conn, …)`; `pending_reference_count(pool, build_id) -> Result<i64>`; `write_band_reference(conn, build_id, rows: &[BandReferenceRow])`; `load_band_reference(pool, band, tc) -> Result<Option<(Uuid, BandReference)>>` (a low-sample cell falls back to the nearest band with the same time class that is not low-sample, else `None`).

- [ ] **Step 1: Write the failing tests** (`#[sqlx::test]`; TS M2 Task 11 explorer cases applied at the storage level, plus): flushing three B2 blitz games (`1.e4 e5` 1-0, `1.e4 c5` 0-1, `1.d4 d5` ½) twice adds counts (`e2e4 {white 2, draws 0, black 2}` after two flushes); `prune_explorer(…, 2)` after one flush removes `d2d4`; `activate_build` supersedes and deletes the old build's rows (count 0), and `query_explorer` reads the new one; `in_theory` false at position total 49; `failed_build_keeps_previous_active`; `unverified_build_is_never_returned_as_active`; `load_band_reference` falls back from a low-sample B3 cell to B2 and returns `None` when every cell is low-sample.
- [ ] **Step 2: Run** `LANE=r2-p2 $RUST cargo test -p refdata`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(refdata): reference builds, explorer and band reference storage`.

### Task 18 (P2): Repertoire tree, struggle detection and verdicts

**Files:**
- Create: `backend/crates/profile-core/src/openings/{repertoire.rs,lines.rs,verdicts.rs}`, `backend/crates/refdata/src/repertoire.rs`, `docs/decisions/ADR-R2-8-repertoire-and-verdicts.md`
- Test: `backend/crates/profile-core/tests/repertoire.rs`, `backend/crates/refdata/tests/repertoire.rs`

**Interfaces:**
- Consumes: Tasks 16, 17 (`load_explorer_index`, `in_theory`, `ExplorerMove`), Task 11 `Z80`, Task 7 `expected_score`.
- Produces (pure, `profile-core`; rules and thresholds exactly TS M2 Task 13): `RepertoireGame`, `RepertoireNode`, `build_repertoire_tree(games, color) -> HashMap<String, RepertoireNode>` (keyed by fen_key, transpositions merge); `book_exit(plies, index, player) -> (Option<u32>, Option<u32>)`; `StruggleReason`; `STRUGGLE_THRESHOLDS`; `LineSummary` (carries every `coaching_input::OpeningVerdict` input field: `line_key, name, eco, color, kind, moves_san, games, score, perf_vs_expected, avg_eval_white_cp: Option<i32>, deviation_ply, deviation_move_san, alternatives: Vec<AlternativeMove { uci, san, name: Option<String>, band_score, games }>`, plus `struggles`, `fix_spot: Option<FixSpot { fen_key, uci, games }>`, `choice_fen_key`); `summarise_lines(games, index, band, tc) -> Vec<LineSummary>`; `detect_struggles(line) -> Vec<StruggleReason>`; `decide_verdict(line, struggles, color_games, alternatives) -> (Option<Verdict>, String)` (rules 1–6 of TS M2; faced lines never `Replace`).
- Produces (DB, `refdata::repertoire`): `rebuild_repertoire(pool, account_id, tc) -> Result<usize /* lines */>` — fills missing `game_openings` rows for imported games (all statuses, including `imported`), loads the explorer index for the player's band (R-4), and replaces `repertoire_nodes` and `opening_lines` for the account × time class in one transaction.

- [ ] **Step 1: Write the failing tests** (TS M2 Task 13): 10 white Italian games (6/2/2, expected 0.5) → node `games 10, score 0.7, perf 0.2`, verdict `Keep`; transposition node `games 2`, two parents; 8 Caro-Kann games with the same `(fen_key, uci)` ΔW 12 in 3 games → `RepeatedDeviationError`, `Fix`, `fix_spot.games == 3`; 15 chosen games score 0 expected 0.2 → `Replace`, alternatives `[g1f3]` only (b1c3 total 90 excluded); same faced → `Learn`; 7 games → `None`. DB: seed 12 imported games, run `rebuild_repertoire` twice → equal row counts and `opening_lines` holds the expected line key.
- [ ] **Step 2: Run** `$RUST cargo test -p profile-core --test repertoire` and `$RUST cargo test -p refdata --test repertoire`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-8** (TS M2's ADR 0041 points: FEN-keyed DAG, lines = `line_family` of the deepest named position, the theory definition, thresholds, faced lines never replaced, playstyle-clash replacement deferred; plus R-7 line keys and R-8 eval averaging). Commit `feat: repertoire tree, struggle detection and opening verdicts`.

---

### Task 19 (D): `lichess-db` — streamed download, SHA-256, zstd, split, headers, sampling

**Files:**
- Create: `backend/crates/lichess-db/src/{config.rs,download.rs,split.rs,headers.rs,sample.rs,movetext.rs}`, `backend/crates/lichess-db/src/main.rs` (subcommand `scan`), `backend/fixtures/lichess-db/sample.pgn` (5 games, < 10 KB)
- Test: `backend/crates/lichess-db/tests/{stream.rs,headers.rs,sample.rs}`
- Create: `docs/decisions/ADR-R2-9-lichess-db-data-pipeline.md`

**Interfaces:**
- Consumes: R1 T2 `classify_time_control`; R1 T8 `user_agent`.
- Produces:
  - `DumpSource { base_url: String /* "https://database.lichess.org" */, month: String }` with `file_name() -> String` (`lichess_db_standard_rated_<month>.pgn.zst`), `url()`, `sums_url()` (`<base>/standard/sha256sums.txt`);
  - `fetch_expected_sha(client: &reqwest::Client, src: &DumpSource) -> Result<String, DumpError>`;
  - `DumpStream` = `open_dump(client, src) -> Result<(impl Stream<Item = Result<String /* one game's PGN */, DumpError>>, ShaHandle), DumpError>`. Bytes flow `reqwest bytes_stream` → a pass-through that updates a `sha2::Sha256` → `tokio_util::io::StreamReader` → `async_compression::tokio::bufread::ZstdDecoder` → line splitter. `ShaHandle::finish(self) -> String` returns the hex digest once the body is fully read; `drain_to_end(stream)` reads the remainder without parsing (R-9);
  - `DumpError { Http(u16), Transport(String), Decode(String), ChecksumMismatch { expected, actual }, SumsMissing(String) }`;
  - `split_games(lines) -> games` rule: a new game starts at a line beginning `[Event ` after a blank line;
  - `DumpHeader { site, lichess_game_id, white, black, white_elo: Option<i32>, black_elo, time_control, base_seconds: Option<u32>, increment_seconds: Option<u32>, result, termination: Option<String>, utc: DateTime<Utc>, has_eval: bool, has_clock: bool }`; `parse_dump_headers(pgn: &str) -> Result<DumpHeader, DumpError>`;
  - `SampleFilter { max_games: Option<u64>, sample_rate: f64 /* 1.0 */, seed: u32, require_clock: bool, time_classes: Option<Vec<TimeClass>> }`; `keep(h: &DumpHeader, f: &SampleFilter) -> Option<TimeClass>` (skips `TimeControl "-"`, terminations `Abandoned`/`Rules infraction`, missing Elo; keeps when `fnv1a32(site ‖ seed) / 2^32 < sample_rate`);
  - `movetext_sans(pgn: &str, max_ply: usize) -> Vec<&str>` (first `max_ply` SAN tokens; skips `{…}`, `;` comments, `(…)` at any depth, NAGs, move numbers, results).
- `main.rs`: `lichess-db scan --month <YYYY-MM> [--base-url URL] [--max-games N]` prints games seen/kept and the verified digest (a smoke tool; Tasks 20–22 add `build`, `bench`, `syzygy`).

- [ ] **Step 1: Write the failing tests** (wiremock serves `sample.pgn` compressed **in the test** with `async_compression` at level 3, plus a `sha256sums.txt` line `<hex>  lichess_db_standard_rated_2026-10.pgn.zst`):
  - `streams_five_games_and_verifies_sha` (5 strings, each starting `[Event `, `finish() == expected`);
  - `sha_mismatch_is_reported_after_the_stream` (a wrong sums line → `ChecksumMismatch` from the verify step, with both digests);
  - `chunk_boundaries_do_not_split_games` (wiremock body sent in 7-byte chunks gives the same 5 games);
  - `drain_after_max_games_still_hashes_whole_file` (`max_games 2`: 2 games parsed, digest still matches);
  - `nothing_is_written_to_disk` (the test's `DATA_DIR` temp directory is empty afterwards);
  - headers: `TimeControl "180+2"` → base 180, inc 2; `lichess_game_id` from `Site`; `has_clock`/`has_eval` true for the game with `[%clk` / `[%eval`;
  - sampling: `require_clock` drops the clockless game; rate 0 keeps none; seed 7 gives the same subset twice; `max_games 2` stops after 2;
  - `movetext_sans` on `1. e4 {[%clk 0:03:00]} (1. d4 d5) 1... e5 $1 2. Nf3 *` with `max_ply 3` → `["e4", "e5", "Nf3"]`.
- [ ] **Step 2: Run** `LANE=r2-d $RUST cargo test -p lichess-db`. Expected: FAIL. Implement (streams only; never buffer the body or write it to disk). Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-9** (the dump is never stored; streamed SHA-256 against `sha256sums.txt`; whole-file verification R-9; one month per run, configurable; hashed usernames with a per-build in-memory salt; operator CLI R-11; quarterly schedule). Commit `feat(lichess-db): streamed dump download with sha-256, zstd and sampling`.

### Task 20 (D): Parallel parse, opening explorer build and throughput benchmark

**Files:**
- Create: `backend/crates/lichess-db/src/{pipeline.rs,explorer_build.rs,bench.rs}`, `docs/research/r2-lichess-db-benchmark.md`, `docs/decisions/ADR-R2-10-opening-explorer-build.md`
- Modify: `backend/crates/lichess-db/src/main.rs` (subcommands `build`, `bench`)
- Test: `backend/crates/lichess-db/tests/{pipeline.rs,explorer_build.rs}`

**Interfaces:**
- Consumes: Task 19; Task 17 (`create_build`, `flush_explorer`, `prune_explorer`, `activate_build`, `fail_build`, `mark_unverified`); Task 1 `band_of`; R1 `Position::play_san`, `fen_key`.
- Produces:
  - `PipelineConfig { threads: usize /* available_parallelism */, batch_games: usize /* 2_000 */, channel_batches: usize /* 2 × threads */ }`; `run_pipeline<S: GameSink>(games: impl Stream<Item = Result<String, DumpError>>, filter: &SampleFilter, sink: &S, cfg: &PipelineConfig) -> Result<PipelineStats, DumpError>` — the async reader fills `Vec<String>` batches into a bounded channel; a rayon pool parses each batch (`par_iter`: headers → `keep` → the sink's `observe`), then the sink's `merge` runs on the async side. `PipelineStats { games_seen, games_kept, bytes_compressed, elapsed: Duration, stage_ms: BTreeMap<String, f64> /* read, parse, aggregate, flush */ }`;
  - `trait GameSink: Sync { type Partial: Send; fn observe(&self, pgn: &str, h: &DumpHeader, tc: TimeClass) -> Self::Partial; fn merge(&self, parts: Vec<Self::Partial>) -> BoxFuture<'_, Result<(), DumpError>>; }`;
  - `explorer_band(white_elo, black_elo) -> Band` (band of the mean rating); `ExplorerSink` (replays only the first `max_ply` SANs; key `fen_key|band|tc|uci`; flushes every `flush_every_games`);
  - `BuildOptions { month, max_games: u64 /* 2_000_000 */, max_ply: usize /* 30 */, min_row_count: i64 /* 3 */, flush_every_games: u64 /* 50_000 */, verify: Verify /* Full | None */, explorer: bool, band_reference: bool /* Task 21 */ }`; `build(pool, client, src, opts) -> Result<BuildOutcome { explorer_build: Option<Uuid>, reference_build: Option<Uuid>, stats }, DumpError>`. Order: create `building` rows → pipeline → drain to end (`Verify::Full`) → compare digest → on match: prune, `activate_build`; on mismatch: `fail_build` (rows deleted, previous active untouched); `Verify::None`: `mark_unverified`. Records one `job_metrics` row (`kind = "lichess-db:build"`, stages from `stage_ms`).
  - `bench(source: BenchSource /* Synthetic { games: u64 } | Http(DumpSource) */, threads: &[usize]) -> Vec<BenchRow { threads, games, games_per_sec, mb_per_sec, stage_ms }>`. `Synthetic` repeats the five fixture games with distinct `Site` values through a zstd encoder in memory, so it needs no network.
- `main.rs`: `lichess-db build --month M [--max-games N] [--explorer] [--band-reference] [--verify full|none]` and `lichess-db bench [--synthetic N | --month M --max-games N] [--threads 1,4,0]` (0 = all cores), printing a Markdown table.

- [ ] **Step 1: Write the failing tests** (`#[sqlx::test]` + wiremock): three B2 blitz games (TS M2 Task 11) with `max_ply 2, min_row_count 1` → start position `e2e4 {1, 0, 1, total 2}` before `d2d4 {total 1}`; `min_row_count 2` → no `d2d4`; a second build supersedes the first; `sha_mismatch_marks_build_failed_and_keeps_previous_active`; `verify_none_leaves_build_unverified`; `pipeline_is_deterministic_across_thread_counts` (1 vs 4 threads → identical explorer rows); `failed_stream_marks_build_failed` (wiremock closes mid-body → status `failed`, previous active unchanged); `build_records_stage_metrics`.
- [ ] **Step 2: Run** `LANE=r2-d $RUST cargo test -p lichess-db`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Benchmark.** Build release (`$RUST cargo build --release -p lichess-db`) and run `lichess-db bench --synthetic 500000 --threads 1,4,0` and, once with network (not a test), `lichess-db bench --month 2026-10 --max-games 1000000 --threads 0`. Expected: the table prints; games/s scales with threads until bounded by decompression or the network (the tool reports which stage is the maximum). Record the table, the host CPU and the command in `docs/research/r2-lichess-db-benchmark.md`.
- [ ] **Step 4: Write ADR-R2-10** (TS M2 ADR 0035's parameters: one month, 2M games, ply ≤ 30, prune < 3, bands by mean rating, 4 time classes (correspondence excluded), quarterly; plus single-pass parallel pipeline, deterministic merges, activation only after verification). Commit `feat(lichess-db): parallel explorer build with throughput benchmark`.

### Task 21 (D): Reference-player selection (bottom-k) and reference games

**Files:**
- Create: `backend/crates/lichess-db/src/reference_select.rs`
- Modify: `backend/crates/lichess-db/src/explorer_build.rs` (wire `band_reference` into `build`)
- Test: `backend/crates/lichess-db/tests/reference_select.rs`
- Create: `docs/decisions/ADR-R2-11-band-reference-method.md` (Task 32 appends)

**Interfaces:**
- Consumes: Tasks 19, 20 (`GameSink`), Task 17 (`create_build`, `insert_reference_games`), R1 T13 `Queue::enqueue`, Task 1 `KIND_REFERENCE_GAME`, `REFERENCE_PRIORITY`, `refgame_key`.
- Produces: `SelectOptions { players_per_cell: usize /* 20 */, games_per_player: usize /* 10 */, oversample: usize /* 20 */, max_scan_games: u64 /* 3_000_000 */ }`; `ReferenceSink::new(salt: [u8; 32], opts)` implementing `GameSink`; `ReferenceSink::finish(self) -> Vec<SelectedGame { player_hash: String, band, tc, pgn, header }>`; `enqueue_reference_games(pool, queue, build_id, games) -> Result<usize>` (inserts `reference_games` with status `pending` after verification, then enqueues one job per game with priority 200, dedupe `refgame:<id>`).
- Algorithm (R-10): for each side of each kept game, `h = sha256(salt ‖ lower(name))`, cell = `(band_of(rating at game), tc)`. Per cell keep a max-heap of at most `players_per_cell × oversample` players with the smallest `h`; each held player buffers at most `games_per_player` games (PGN + header). A player evicted from the heap never re-enters (its hash is fixed). At the end, per cell pick the `players_per_cell` smallest hashes with ≥ `games_per_player` buffered games; a game selected for both colours is inserted once and observed for both. Memory bound: cells × players × games × PGN size, logged at start.

- [ ] **Step 1: Write the failing tests:** 30 generated PGNs from 3 players in one cell (`players_per_cell 2, games_per_player 5`) → 10 selected games from exactly 2 players; the same salt and input give the same selection, a different salt may differ; `eviction_is_final` (a player evicted by smaller hashes is absent even if it later reaches 10 games); `usernames_are_never_stored` (no `reference_games` column contains `"alice"`); `enqueue_reference_games` inserts 10 rows and 10 live `analyse-reference-game` jobs at priority 200; `selection_waits_for_verification` (on SHA mismatch no `reference_games` rows exist).
- [ ] **Step 2: Run** `$RUST cargo test -p lichess-db --test reference_select`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-11** first part (player-sampled cells 20 × 10 per band × time class, both colours, single-pass bottom-k with oversample 20, hashed names, per-build salt never stored). Commit `feat(lichess-db): bottom-k reference player selection`.

### Task 22 (D): Syzygy 3-4-5 fetch

**Files:**
- Create: `backend/crates/lichess-db/src/syzygy.rs`, `backend/crates/lichess-db/data/syzygy-345.sha256` (manifest)
- Modify: `backend/crates/lichess-db/src/main.rs` (subcommand `syzygy`)
- Test: `backend/crates/lichess-db/tests/syzygy.rs`

**Interfaces:**
- Consumes: Task 19 HTTP client setup.
- Produces: `fetch_syzygy(client, base_url: &str /* "https://tablebase.lichess.ovh/tables/standard/3-4-5" */, manifest: &Manifest, dest: &Path) -> Result<FetchSummary { files, bytes, skipped }, DumpError>` (stream each file to `<name>.part` while hashing, rename only on match, delete `.part` on mismatch; skip files already present with a matching hash); `Manifest::parse(text) -> Manifest` (`<sha256>  <file>` lines). CLI: `lichess-db syzygy [--dest $DATA_DIR/syzygy]`.

- [ ] **Step 1: Build the manifest.** List the mirror (`curl -s <base_url>/`). If it publishes checksums, use them; otherwise download once and record the SHA-256s (trust on first use, noted in ADR-R2-12 by Task 23). Commit the manifest only.
- [ ] **Step 2: Write the failing tests** (wiremock): a 2-file manifest writes both files; a corrupted body gives `ChecksumMismatch` and leaves no file and no `.part`; a re-run makes 0 body requests.
- [ ] **Step 3: Run** `$RUST cargo test -p lichess-db --test syzygy`. Expected: FAIL. Implement. Rerun. Expected: PASS. Then run `$RUST cargo run --release -p lichess-db -- syzygy` once (about 1 GB into `data/syzygy`; network, not a test). Commit `feat(lichess-db): verified syzygy 3-4-5 fetch`.

---

### Task 23 (E): Syzygy probe pool

**Files:**
- Create: `backend/crates/enrich/src/tablebase/{mod.rs,wdl.rs,pool.rs,cached.rs}`, `docs/decisions/ADR-R2-12-local-syzygy-probe.md`
- Test: `backend/crates/enrich/tests/tablebase.rs`

**Interfaces:**
- Consumes: R1 T6 `UciEngine::start(StartOptions { syzygy_path, .. })`, `AnalyseOptions`; Task 1 `TablebaseProbe`; Task 4 `piece_count`; Task 3 `position_tb`.
- Produces: `wdl_from_lines(lines: &[AnalysisLine]) -> Option<Wdl>` (mate > 0 or cp ≥ 19 000 → 1; mate ≤ 0 or cp ≤ −19 000 → −1; other cp → 0; empty → `None`); `SyzygyPool::start(stockfish: &Path, syzygy_path: &Path, size: usize /* 1 */, nodes: u64 /* 1000 */) -> Result<SyzygyPool, EngineError>` implementing `TablebaseProbe` (`max_pieces 5`; > 5 pieces → `None` without an engine call; a directory without `.rtbw` files → every probe `None` and one `tracing::warn!`; engines are its own `UciEngine`s, never R1's `EnginePool`); `CachedTbProbe::new(inner: Arc<dyn TablebaseProbe>, pool: PgPool)` (reads/writes `position_tb` by `fen_key`).

- [ ] **Step 1:** Check expected values against the Lichess tablebase API once (TS M2 Task 20 Step 1): `8/4k3/8/4K3/4P3/8/8/8 w` → draw, the same with `b` → loss, `8/8/8/8/8/4k3/8/KQ6 w` → win.
- [ ] **Step 2: Write the failing tests:** `wdl_from_lines` on cp 19980 → 1, −19990 → −1, 35 → 0, mate 3 → 1, mate 0 → −1, empty → `None`; a 6-piece FEN makes 0 engine calls; `missing_tablebase_files_returns_none` (empty temp dir); Docker integration (real Stockfish and `SYZYGY_PATH`; **fails** under `IN_DOCKER=1` if `KPvK.rtbw`/`KQvK.rtbw` are missing): the three FENs give `0, -1, 1`; the cached probe answers the second lookup without the engine; `main_pool_never_sets_syzygy_path` (an R1 `EnginePool` started with a fake Stockfish script that logs stdin never receives `setoption name SyzygyPath`).
- [ ] **Step 3: Run** `LANE=r2-e $RUST cargo test -p enrich --test tablebase`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 4: Write ADR-R2-12** (separate engines so `sf19-n300k-v1` is unchanged, `cp ±20000` parsing, 5 pieces locally, 6–7 via the API only in drills (M6), manifest trust). Commit `feat(enrich): local syzygy probe pool`.

### Task 24 (E): Maia licence check, difficulty providers and `services/maia`

**Files:**
- Create: `backend/crates/enrich/src/difficulty/{mod.rs,spread.rs,maia.rs,select.rs}`, `docs/decisions/ADR-R2-13-maia-licence-and-difficulty.md`
- Create (branch A only): `services/maia/{pyproject.toml,Dockerfile,app/__init__.py,app/main.py,app/model.py,tests/test_api.py}`; Modify `docker-compose.yml` (service `maia`, profile `maia`, port `127.0.0.1:58080:8080`; `rust-worker` env `MAIA_URL: ${MAIA_URL:-}`)
- Test: `backend/crates/enrich/tests/difficulty.rs`

**Interfaces:**
- Consumes: Task 1 `DifficultyProvider`, `DifficultyInput`.
- Produces: `SpreadDifficulty` (`model "spread-v1"`; `None` without ≥ 2 lines; `playable` = lines with `win ≥ best − 5`; `p_ok = playable / lines`; `p_best = 1 / playable`); `MaiaDifficulty::connect(url: &str, timeout: Duration /* 10 s */, batch_size: usize /* 64 */) -> Result<MaiaDifficulty, DifficultyError>` (model id from `GET /healthz`; `POST /v1/predict {items:[{fen, eloSelf, eloOppo}]}` → `{items:[{probs:{uci:p}} | {error:"invalid_fen"}]}`; `p_best = probs[best] or 0`; `p_ok = Σ probs` over known lines with `win ≥ best − 5`; transport errors and 5xx → `Unavailable`); `select_difficulty_provider(maia_url: Option<&str>) -> Arc<dyn DifficultyProvider>` (Maia when set and `/healthz` answers, else spread).

- [ ] **Step 1: Licence check (first).** Exactly TS M2 Task 21 Step 1 (`gh api repos/CSSLab/maia2 --jq '.license.spdx_id'`, PyPI metadata, the README's terms for the **weights**, runtime dependency licences; python-chess GPL-3.0 is acceptable only as an unmodified, separate, undistributed server process). Compatible → branch A (Steps 2–6). Incompatible (non-commercial, unlicensed, AGPL pending the owner) → branch B: Steps 2–3 and 6 for the spread provider only, no `services/maia`, `MAIA_URL` unset. Record the evidence in ADR-R2-13.
- [ ] **Step 2: Write the failing tests** (wiremock for Maia): spread `[60, 58, 40]` → `(0.5, 2/3)`; `[60, 30, 20]` → `(1.0, 1/3)`; one line → `None`; Maia `probs {e2e4 0.5, d2d4 0.3, g1f3 0.2}`, lines `[(e2e4,55),(d2d4,52),(g1f3,40)]`, best e2e4 → `(0.5, 0.8)`; 130 inputs → 3 requests; a 503 → `Unavailable`; `select_difficulty_provider(None).model() == "spread-v1"`; an unreachable URL also gives spread.
- [ ] **Step 3: Run** `$RUST cargo test -p enrich --test difficulty`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 4 (A): Write `services/maia`** as TS M2 Task 21 Step 5 (FastAPI, model loaded once, `MAIA_MODEL` rapid/blitz, Elo clamped, ≤ 256 items, per-item `invalid_fen`, `GET /healthz` → `{model: "maia2-<type>-<version>"}`, pytest with an injected fake model: probabilities sum to 1, invalid FEN, 257 items → 422; `python:3.12-slim`, pinned versions, weights downloaded at image build, never into the repo).
- [ ] **Step 5 (A): Run** `docker compose --profile maia up -d --build maia`, `docker compose --profile maia run --rm maia pytest`, and from `rust-dev` `curl -s http://maia:8080/healthz`. Expected: tests pass and the model id prints; the start position's top move is one of `e2e4, d2d4, g1f3, c2c4`. Add `maia_live_contract` to `enrich/tests/difficulty.rs`, gated by `MAIA_URL` being set (it fails, not skips, when `IN_DOCKER=1` and `MAIA_URL` is set but unreachable).
- [ ] **Step 6: Commit** `feat(enrich): difficulty providers with maia licence gate` (plus `feat(maia): maia-2 difficulty service` in branch A).

### Task 25 (E): Enrichment pass (MultiPV, PV completion, threat probe, TB, difficulty)

**Files:**
- Create: `backend/crates/enrich/src/{protocol.rs,critical.rs,store.rs,enrich_game.rs,persist.rs}`, `docs/decisions/ADR-R2-14-enrichment-protocol.md`
- Test: `backend/crates/enrich/tests/{critical.rs,enrich_game.rs,persist.rs}`

**Interfaces:**
- Consumes: R1 `Analyser`, `AnalyseOptions`, `ENGINE_NODES`, `PositionEval`, `MoveRecord`, `ParsedGame`, `testkit::FakeAnalyser`; Task 4 `null_move_fen`, `piece_count`; Task 1 types; Tasks 23–24 only through their traits.
- Produces:
  - `protocol.rs`: `ENRICHMENT_PROTOCOL = "enr-v1"`, `MULTIPV_PROTOCOL = "sf19-n300k-mpv3-v1"`, `THREAT_PROTOCOL = "sf19-n50k-null-v1"`, `THREAT_NODES = 50_000`, `MULTIPV = 3`, `MAX_CRITICAL_PLIES = 30`;
  - `select_critical_plies(records: &[MoveRecord], colors: &[Color], max: usize) -> Vec<u32>` (ΔW ≥ 5, or the previous ply's ΔW ≥ 10; over `max` keep the highest ΔW, ties → lower ply; ascending);
  - `trait EnrichmentStore: Send + Sync { get_multipv(keys) -> BoxFuture<Result<HashMap<String, Vec<MultipvLine>>>>; put_multipv(&[(String, Vec<MultipvLine>)]); get_threat(keys) -> …HashMap<String, (EngineScore, Vec<String>)>; put_threat(…) }`, `DbEnrichmentStore::new(pool)`, `MemoryEnrichmentStore::default()`;
  - `EnrichDeps { analyser: Arc<dyn Analyser>, store: Arc<dyn EnrichmentStore>, tb: Option<Arc<dyn TablebaseProbe>>, difficulty: Option<Arc<dyn DifficultyProvider>>, cancel: CancellationToken }`;
  - `EnrichInput<'a> { parsed: &'a ParsedGame, evals: &'a [PositionEval], records: &'a [MoveRecord], colors: &'a [Color], elo_white: Option<i32>, elo_black: Option<i32> }`;
  - `enrich_game(deps: &EnrichDeps, input: &EnrichInput) -> Result<Enriched { raw: Vec<Option<RawMoveFeatures>>, difficulty_model: Option<String>, stages: BTreeMap<String, f64> /* multipv_ms, threat_ms, tb_ms, difficulty_ms */ }, EnrichError /* Engine(EngineError) | Cancelled | Store(sqlx::Error) */>`. Rules exactly TS M2 Task 22 (MultiPV 3 at `ENGINE_NODES` on critical plies' `fen_before`; `refutation_pv = evals[i+1].pv`, or for critical plies with an empty PV a MultiPV probe of `fen_after`; threat at `THREAT_NODES` on `null_move_fen(fen_before)` for eligible plies of requested colours, skipped when `None`; TB when ≤ 5 pieces with `tb_wdl_after = −probe(fen_after)`; difficulty batched for requested eligible plies; a difficulty error → all `None` and one warning; never writes `position_evals`; every FEN validated with `Position::from_fen` before reaching the analyser);
  - `persist_move_features(conn: &mut PgConnection, game_id: Uuid, records: &[MoveRecord], raw: &[Option<RawMoveFeatures>], derived: Option<&[MoveFeatures]>) -> Result<(), sqlx::Error>` (delete then insert in chunks of 500 with `UNNEST`).

- [ ] **Step 1: Write the failing tests** (FakeAnalyser recording `(fen, nodes, multipv)`, memory store), TS M2 Task 22's cases: `select_critical_plies` ΔW `{3:6, 4:0, 5:12, 6:2, 7:1}` → `[3,5,6]`, max 2 → `[3,5]`; MultiPV calls are exactly the critical player plies' `fen_before` at `{300_000, 3}`, and a second run makes 0 MultiPV calls; the threat probe uses the flipped side at `{50_000, 1}`; `threat_probe_skipped_when_in_check`, and for decided and opponent plies; `refutation_pv_falls_back_to_engine_when_cached_pv_is_empty`; the TB fake is called only for ≤ 5 pieces and `tb_wdl_after` is negated; `difficulty_provider_failure_yields_null_difficulty` (all `None`, `difficulty_model None`); `enrichment_never_touches_position_evals` (DB: row count unchanged); `cancel_returns_cancelled_within_2s` (hanging fake); `persist_replaces_rows` (a re-run leaves the same count); `stages_are_reported` (four keys).
- [ ] **Step 2: Run** `$RUST cargo test -p enrich`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-14** (TS M2 ADR 0030's content: the enrichment layer and ids, MultiPV never replaces the MultiPV-1 eval, critical-ply selection and its bias, cost about +25–40% CPU, bumping `ENRICHMENT_PROTOCOL` re-enriches from the cache). Commit `feat(enrich): enrichment pass with multipv, threat probe and tablebase`.

---

### Task 26 (S): Tier selection rules (pure)

**Files:**
- Create: `backend/crates/scheduler/src/{select.rs,priority.rs,precision.rs}`
- Test: `backend/crates/scheduler/tests/select.rs`

**Interfaces:**
- Consumes: Task 1 constants.
- Produces (values exactly TS M2 Task 23): `Plan { Free, Pro }`; `TierLimits { import_max, p0, p1, p2: bool, p3_per_week }` with `tier_limits(Plan)` (free 1000/50/0/false/20; pro 10 000/200/100/true/300); `P2_PER_MONTH = 15`, `P2_MAX_MONTHS = 36`, `P1_MIN_RECENT = 20`, `RECENT_DAYS = 90`, `MIN_PLIES = 10`, `BATCH_SIZE = 25`, `PREVIEW_GAMES = 20`; `CandidateGame { id, time_class, played_at, created_at, rated, ply_count: Option<i32>, has_lichess_evals, status: AnalysisStatus, tier: Option<i16> }`; `primary_time_class(games, now) -> Option<TimeClass>`; `active_time_classes(games, now) -> Vec<TimeClass>`; `SelectInput<'a> { games, plan, now, first_scheduled_at, p3_selected_last_7d, stopped: &'a HashSet<TimeClass>, in_flight_backfill: &'a HashMap<TimeClass, u32> }`; `Selected { game_id, tier: u8, priority: i16 }`; `select_for_analysis(i: &SelectInput) -> Vec<Selected>`; `job_priority(tier: u8, plan: Plan, has_lichess_evals: bool) -> i16` = `BASE·2 + (if has_lichess_evals {0} else {1})` with BASE t0 {pro 1, free 2}, t3 {pro 3, free 4}, t1 5, t2 10; `fair_tier(priority: i16) -> i16 = priority / 2`; `PrecisionInput { key, ci_low, ci_high, band_median: Option<f64>, n_unweighted }`; `is_precise(est: &[PrecisionInput]) -> bool`.

- [ ] **Step 1: Write the failing tests**, every TS M2 Task 23 pure case: free 70 blitz + 30 rapid → 50 tier-0 blitz, most recent first; pro 250 blitz, 30 rapid, 40 bullet → 200 tier-0, 25 tier-1 rapid, 0 bullet; plyCount 8 skipped; casual only after rated; pro P2 40 months × 20 → 25 tier-2 from month 11, newest first, nothing older than 36 months; P3 free 30 games after `first_scheduled_at`: 0 → 20, 15 → 5; stopped TC and in-flight backfill → no tier 1/2; `job_priority(0, Pro, true) == 2`, `(0, Free, false) == 5`, `(2, Pro, false) == 21`; `is_precise` width 0.004 median 0.02 → true; width 0.01 n 500 W1 → false; n 1000 → true; empty → false.
- [ ] **Step 2: Run** `LANE=r2-s $RUST cargo test -p scheduler --test select`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(scheduler): tier selection rules`.

### Task 27 (S): Fair round-robin dequeue on the Postgres queue

**Files:**
- Create: `backend/crates/queue/src/strategy.rs`, `backend/crates/scheduler/src/fair.rs`, `docs/decisions/ADR-R2-15-fair-scheduling-and-preview.md`
- Modify: `backend/crates/queue/src/runner.rs` (`RunnerConfig.strategy`), `backend/crates/queue/src/queue.rs` (move R1's `SelectStrategy`/`PriorityFifo` hook into `strategy.rs`), `backend/crates/queue/src/lib.rs`
- Test: `backend/crates/scheduler/tests/fair.rs`, `backend/crates/queue/tests/strategy.rs`

**Interfaces:**
- Consumes: R1 T13 `Queue::dequeue`, R1 T14 `Runner`.
- Produces:
  - `pub trait SelectStrategy: Send + Sync { fn dequeue<'a>(&'a self, queue: &'a Queue, kinds: &'a [&'a str], owner: &'a str, limit: i64, lease: Duration) -> BoxFuture<'a, Result<Vec<Job>, QueueError>>; }` (if R1's trait signature differs, conform it to this one); `PriorityFifo` (calls `Queue::dequeue`, R1 behaviour, the default); `RunnerConfig.strategy: Arc<dyn SelectStrategy>` defaulting to `PriorityFifo` (every R1 test unchanged);
  - `scheduler::fair::FairRoundRobin { slice: i64 /* 5 */, served_window: Duration /* 1 h */ }` implementing `SelectStrategy`;
  - `fair_order_sql()` shared with Task 30 (the ordering CTE without the lock).
- Algorithm (R-12; one statement, the lock taken after ordering because `FOR UPDATE` cannot share a level with window functions):

```sql
WITH ready AS (
  SELECT j.id, j.user_key, j.priority, j.run_at,
         row_number() OVER (PARTITION BY j.user_key ORDER BY j.priority, j.run_at, j.id) AS rn
  FROM jobs j
  WHERE j.state = 'queued' AND j.kind = ANY($1) AND j.run_at <= now()),
served AS (
  SELECT account_id AS user_key, count(*) AS n FROM games
  WHERE analysis_started_at > now() - $5 * interval '1 millisecond' GROUP BY account_id),
ordered AS (
  SELECT r.id, r.priority / 2 AS tier, (coalesce(s.n, 0) + r.rn - 1) / $6 AS slice,
         coalesce(a.created_at, 'infinity'::timestamptz) AS joined_at, r.priority, r.run_at
  FROM ready r LEFT JOIN served s ON s.user_key = r.user_key
               LEFT JOIN linked_accounts a ON a.id = r.user_key
  ORDER BY tier, slice, joined_at, r.priority, r.run_at, r.id
  LIMIT $2 * 4),
locked AS (
  SELECT j.id FROM jobs j JOIN ordered o ON o.id = j.id
  WHERE j.state = 'queued' FOR UPDATE OF j SKIP LOCKED),
pick AS (
  SELECT o.id FROM ordered o JOIN locked l ON l.id = o.id
  ORDER BY o.tier, o.slice, o.joined_at, o.priority, o.run_at, o.id LIMIT $2)
UPDATE jobs j SET state = 'running', attempts = j.attempts + 1, lease_owner = $3,
  lease_token = gen_random_uuid(), lease_until = now() + $4 * interval '1 millisecond',
  heartbeat_at = now(), started_at = now()
FROM pick WHERE j.id = pick.id
RETURNING j.*;
```

  Jobs with `user_key NULL` (reference games) form one pseudo-user with `joined_at = infinity`.

- [ ] **Step 1: Write the failing tests** (`#[sqlx::test]`, accounts with explicit `created_at`, jobs enqueued through `Queue::enqueue` with priorities from `job_priority`):
  - `round_robin_in_slices_of_5`: accounts A (joined first) and B, 20 P0 jobs each; dequeue one at a time (`limit 1`, each completed and its game's `analysis_started_at` set to now) → users in order `A×5, B×5, A×5, B×5, …`;
  - `newcomer_gets_first_10_before_anyone_gets_11th`: A has 10 served in the last hour and 40 queued; B joins with 50 → the next 10 claims are all B's, then A's 11th;
  - `tier_orders_before_slice`: A's queued P2 jobs (priority 20) never precede B's P0 jobs;
  - `reference_jobs_come_last` (priority 200, `user_key` NULL);
  - `concurrent_fair_dequeue_is_disjoint` (2 tasks × `limit 5` over 10 jobs → disjoint sets of 5);
  - `fair_dequeue_scales` (50 000 queued jobs over 500 accounts → one `dequeue(limit 16)` under 1 s);
  - `queue/tests/strategy.rs`: `default_strategy_is_priority_fifo` (R1 order unchanged).
- [ ] **Step 2: Run** `$RUST cargo test -p scheduler --test fair` and `$RUST cargo test -p queue`. Expected: FAIL. Implement. Rerun. Expected: PASS (all R1 queue tests too).
- [ ] **Step 3: Write ADR-R2-15** first part (§8d.1 as implemented: tier, slice of 5, join time; `served` from `games.analysis_started_at` because finished jobs are pruned; the lock-after-order statement; the pseudo-user for reference games; R-12). Commit `feat(scheduler): fair round-robin dequeue strategy`.

### Task 28 (S): Preview pass at 30k nodes

**Files:**
- Create: `backend/crates/scheduler/src/preview.rs`
- Test: `backend/crates/scheduler/tests/preview.rs`
- Modify: ADR-R2-15 (append "Preview pass")

**Interfaces:**
- Consumes: R1 `parse_pgn`, `build_move_records`, `game_accuracy`, `win_percent`, `flip_score`, `Analyser`, `events::publish` (`GameAnalysed`), `queue::Handler`; Task 1 `KIND_PREVIEW`, `PreviewPayload`, `preview_key`, `PREVIEW_PRIORITY`; Task 3 `game_previews`, `record_stage_metrics`.
- Produces: `PREVIEW_NODES: u64 = 30_000`, `PREVIEW_PROTOCOL = "sf19-n30k-preview-v1"`; `PreviewDeps { pool, analyser: Arc<dyn Analyser>, concurrency: usize /* 2 */ }`; `handle_preview(deps, job, payload, cancel) -> Result<PreviewOutcome /* Written | SkippedDone | SkippedUnsupported */, JobError>`; `PreviewHandler` (implements `Handler` for `KIND_PREVIEW`); `enqueue_previews(queue, account_id, game_ids: &[Uuid]) -> Result<usize>` (dedupe `preview:<game>`, priority 0, `user_key` = account, max_attempts 2).
- Rules (R-13, §8d.2): skip when the game is already `done`; evaluate every position at 30k nodes, MultiPV 1, deduplicated by `fen_key` within the game, **no cache read or write**, terminal positions as R1 T18; build records; write `game_previews` (protocol, both accuracies, `moves` = `[{ply, judgement, deltaW, accuracy}]`) with `ON CONFLICT (game_id) DO UPDATE`; after commit publish `game.analysed {gameId, accuracy, preview: true}`; record `job_metrics` (`kind "preview-game"`, stages `parse_ms`, `engine_wall_ms`, `persist_ms`). Never touches `games.analysis_status`, `moves` or `position_evals`.

- [ ] **Step 1: Write the failing tests** (`#[sqlx::test]`, FakeAnalyser): `preview_uses_30k_nodes_multipv_1` (every call `(30_000, 1)`); `preview_never_writes_position_evals_or_game_metrics` (counts of `position_evals`, `moves`, `game_metrics`, `weakness_series_point` unchanged; game status unchanged); `preview_event_is_marked` (`payload.preview == true`); `done_game_is_skipped`; `rerun_overwrites_one_row`; `enqueue_previews_dedupes` (twice → 20 live jobs for 20 games).
- [ ] **Step 2: Run** `$RUST cargo test -p scheduler --test preview`. Expected: FAIL. Implement. Rerun. Expected: PASS. Append the "Preview pass" section to ADR-R2-15 (the only exception to one protocol; never cached; never used for KPIs, posteriors or timeline; replaced by the full pass, which deletes the row in Task 31). Commit `feat(scheduler): 30k-node preview pass`.

### Task 29 (S): `schedule-analysis` job, `imported` status and import/reconcile integration

**Files:**
- Create: `backend/crates/scheduler/src/schedule_job.rs`, `docs/decisions/ADR-R2-16-tiered-analysis-scheduler.md`
- Modify: `backend/crates/analysis/src/jobs/import.rs`, `backend/crates/analysis/src/jobs/reconcile.rs`, `backend/crates/analysis/src/jobs/analyse.rs` (only `enqueue_analyse` gains a priority variant)
- Test: `backend/crates/scheduler/tests/schedule_job.rs`; update `backend/crates/analysis/tests/{import_account.rs,import_resume.rs,reconciler.rs}`

**Interfaces:**
- Consumes: Tasks 26–28; R1 T19 `enqueue_analyse`, T20 `reconcile`, T21 `handle_import_account`; Task 3 columns.
- Produces:
  - in analysis: `enqueue_analyse_with_priority(queue, game_id, account_id, priority: i16) -> Result<bool>` (R1's `enqueue_analyse` keeps priority 100 and calls it);
  - import changes: new games are upserted with `status: Imported` and `ply_count` (counted from SAN tokens), no per-game enqueue; after the run commits, enqueue `KIND_SCHEDULE` (dedupe `schedule:<account>`); the import `max` defaults to `tier_limits(plan).import_max`;
  - reconcile change: re-enqueue a lost pending game with `coalesce(analysis_priority, 100)`; `imported` games are never swept;
  - `ScheduleDeps { pool, queue, now: fn() -> DateTime<Utc> }`; `handle_schedule(deps, payload: SchedulePayload) -> Result<ScheduleSummary { selected: usize, by_tier: [usize; 4], previews: usize }, JobError>` — loads candidates, estimates per time class (for `is_precise` via `weakness_estimates`; stopped TCs), runs `select_for_analysis`, sets `pending`, `analysis_tier`, `analysis_priority`, `selected_at` in one transaction, enqueues after commit with `enqueue_analyse_with_priority`; on the account's first run (`analysis_scheduled_at IS NULL`) sets it and enqueues previews for the first `PREVIEW_GAMES` tier-0 games; `ScheduleHandler` implements `Handler`.

- [ ] **Step 1: Write the failing tests** (TS M2 Task 23 DB cases): an import leaves `imported` rows and one live schedule job; `handle_schedule` sets pending with tier and priority, and the live job's `priority` matches `job_priority`; a re-run selects 0; the reconciler re-enqueues a lost pending game with its stored priority and ignores `imported` games; `first_schedule_enqueues_20_previews` (and 0 on the second run); `cli_style_flow_still_finishes` (import → schedule → analyse with FakeAnalyser → every selected game `done`). Update the R1 import tests that expected `pending` + per-game jobs to expect `imported` + one schedule job.
- [ ] **Step 2: Run** `$RUST cargo test -p scheduler -p analysis`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-16** (TS M2 ADR 0042: `imported` status, the priority table, batch-wise adaptive stopping, `plan` from the payload until M7, rolling 7-day P3 window; plus previews on the first run). Commit `feat(scheduler): schedule-analysis job with imported status`.

### Task 30 (S): Back-pressure ETA, queue lag and autoscaling hooks

**Files:**
- Create: `backend/crates/scheduler/src/{eta.rs,lag.rs}`, `docs/decisions/ADR-R2-17-eta-and-autoscaling-hooks.md`
- Test: `backend/crates/scheduler/tests/{eta.rs,lag.rs}`

**Interfaces:**
- Consumes: Task 27 `fair_order_sql`; R1 `job_metrics`.
- Produces:
  - `AccountEta { done: i64, total: i64, queued_ahead: i64, games_per_sec: Option<f64>, eta: Option<Duration> }`; `account_eta(pool, account_id) -> Result<AccountEta>`: `total` = the account's games with `selected_at` set and status `pending | analysing | done`; `done` = those `done`; `queued_ahead` = the position, in the fair order (without locking), of the account's last queued analyse job; `games_per_sec` = `job_metrics` rows of kind `analyse-game` in the last 10 minutes / 600 (`None` under 5 rows); `eta = queued_ahead / games_per_sec`;
  - `format_eta(e: &AccountEta) -> String`: `"about {m} min, analysing {done} / {total}"` with `m = ceil(eta / 60 s)`; `"under 1 min, analysing …"` below 60 s; `"analysing {done} / {total}"` when the ETA is unknown (never a spinner);
  - `QueueLag { oldest_p0_age: Option<Duration>, queued_games: i64, core_seconds_queued: f64 }`; `queue_lag(pool) -> Result<QueueLag>` (P0 = queued analyse jobs with `fair_tier ≤ 2`, preview included; core-seconds = queued games × mean `engine_cpu_ms` per game over the last 24 h, default 12 000 ms);
  - `ScaleConfig { p0_lag_threshold: 3 min, idle_scale_down: 15 min, max_extra_servers: 4, engines_per_server: 48, monthly_cap_eur: f64, eur_per_server_hour: f64 }`; `ScaleAdvice { Add(u32), Hold, Remove(u32) }`; `scale_advice(lag: &QueueLag, idle_for: Duration, current_extra: u32, spent_this_month_eur: f64, cfg: &ScaleConfig) -> ScaleAdvice` (add `min(max − current, max(1, ceil(core_seconds / (engines_per_server × 600))))` when P0 lag > threshold and the cap leaves room for an hour; remove 1 when idle ≥ 15 min and `current > 0`; else hold).

- [ ] **Step 1: Write the failing tests:** `format_eta` with done 38, total 50, eta 170 s → `"about 3 min, analysing 38 / 50"`; 45 s → `"under 1 min, analysing 38 / 50"`; unknown → `"analysing 38 / 50"`; DB: two accounts with queued jobs and 60 `job_metrics` rows in the last 10 minutes (0.1 games/s) → B's `queued_ahead` counts A's earlier slices and `eta ≈ queued_ahead × 10 s`; `queue_lag` with a P0 job queued 4 minutes ago → `oldest_p0_age ≥ 4 min`; `scale_advice`: lag 4 min, 2 000 core-seconds → `Add(1)`; lag 2 min → `Hold`; idle 16 min with 2 extra → `Remove(1)`; cap reached → `Hold`; `current_extra == max` → `Hold`.
- [ ] **Step 2: Run** `$RUST cargo test -p scheduler --test eta --test lag`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-17** (§8d.4 and §8d.5: the measurements used, the advice function as the hook for an external autoscaler, no cloud API in R2, owner-approved caps). Commit `feat(scheduler): back-pressure eta and autoscaling hooks`.

---

### Task 31 (I): Analyse-game hook — features, metrics and series at analysis time

**Files:**
- Create: `backend/crates/analysis/src/hook.rs`, `backend/crates/profile/src/{analyse_features.rs,load.rs,requeue.rs,hook_impl.rs}`
- Modify: `backend/crates/analysis/src/jobs/analyse.rs` (call the hook), `backend/crates/analysis/src/lib.rs`
- Test: `backend/crates/profile/tests/analyse_features.rs`, `backend/crates/analysis/tests/hook.rs`

**Interfaces:**
- Consumes: Tasks 6–10, 16 (`classify_opening`, `line_key`), 17 (`load_explorer_index`, `in_theory`), 18 (`book_exit`), 25 (`enrich_game`, `persist_move_features`), 3 (`profile_rows`, `stage_metrics`); R1 T19 analyse job.
- Produces:
  - `analysis::hook`: `HookInput<'a> { game: &'a GameRow, parsed: &'a ParsedGame, evals: &'a [PositionEval], records: &'a [MoveRecord], cancel: &'a CancellationToken }`; `HookError { Cancelled, Failed(String) }`; `pub trait AnalyseHook: Send + Sync { fn prepare<'a>(&'a self, i: HookInput<'a>) -> BoxFuture<'a, Result<Box<dyn PreparedHook>, HookError>>; }` and `pub trait PreparedHook: Send { fn persist<'a>(&'a self, tx: &'a mut PgConnection) -> BoxFuture<'a, Result<(), sqlx::Error>>; fn after_commit(self: Box<Self>) -> BoxFuture<'static, ()>; }`; `AnalyseGameDeps.hook: Option<Arc<dyn AnalyseHook>>` (`None` keeps R1 behaviour exactly). The analyse job calls `prepare` after `build_move_records`; `HookError::Cancelled` maps to R1's shutdown release; `HookError::Failed` is logged and the game is persisted without hook rows (enrichment_protocol stays `NULL`, so `requeue_unenriched` retries it); `persist` runs inside the moves transaction under the fence; `after_commit` runs after R1's `game.analysed` publish.
  - `profile`: `compute_game_features(i: FeatureInput) -> GameFeatures { enriched, derived: Vec<MoveFeatures>, metrics: PlayerGameMetrics, observation: GameObservation, book_exit_ply: Option<u32>, line_key: Option<String> }` (phases recomputed with `detect_phases(fens, book_exit_ply.unwrap_or(20))`; strengths from `extract_strength_plies`); `ProfileHook { pool, enrich: EnrichDeps, queue }` implementing `AnalyseHook` — `persist` writes derived `move_features`, `game_metrics`, `game_openings`, replaces the game's `weakness_series_point` rows, sets `games.enrichment_protocol = 'enr-v1'` and `analysed_at = now()`, deletes the game's `game_previews` row; `after_commit` enqueues `KIND_BUILD_PROFILE` (dedupe `profile:<account>:<tc>`, `run_at = now() + 30 s`) and records a `job_metrics` row (`kind "enrich-game"`, stages from `Enriched.stages` plus `features_ms`);
  - `load_analysed_games(pool, account_id, tc) -> Result<Vec<(GameContext, PlayerGameMetrics)>>` (score from the player's POV); `load_series_points(pool, account_id, tc) -> Result<BTreeMap<SeriesKey, Vec<SeriesPoint>>>`; `load_exemplar_moves(pool, refs: &[(Uuid, u32)]) -> Result<HashMap<(Uuid, u32), ExemplarRow>>`; `requeue_unenriched(pool, queue) -> Result<usize>` (`done` games with `enrichment_protocol IS DISTINCT FROM 'enr-v1'` → pending at tier 2 and enqueued; main evals come from the cache).

- [ ] **Step 1: Write the failing tests** (`#[sqlx::test]`, FakeAnalyser, memory enrichment store, spread difficulty), TS M2 Task 24 cases: Scholar's mate for an account playing black → one `game_metrics` row with `metrics.version == 1` and `blunders == 1`; a `W1` series row with opportunities equal to black's eligible moves; a re-run keeps the same row counts; `requeue_unenriched` picks a `done` game with `NULL` enrichment protocol and leaves an `enr-v1` game alone; `load_analysed_games` returns player-POV scores. New: `hook_none_keeps_r1_behaviour` (analysis tests unchanged with `hook: None`); `hook_failure_still_marks_done_without_features`; `hook_cancel_releases_job`; `full_pass_deletes_preview_row`; `build_profile_enqueued_once_per_account_and_tc` (two games → one live job).
- [ ] **Step 2: Run** `LANE=r2-i $RUST cargo test -p profile --test analyse_features` and `$RUST cargo test -p analysis`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(profile): analysis-time features, metrics and weakness series`.

### Task 32 (I): Reference-game analysis, aggregation and finalisation

**Files:**
- Create: `backend/crates/profile/src/{reference_jobs.rs,reference_aggregate.rs}`
- Test: `backend/crates/profile/tests/band_reference.rs`
- Modify: ADR-R2-11 (append aggregation)

**Interfaces:**
- Consumes: Task 31 `compute_game_features`; Task 25 `enrich_game`; R1 `parse_pgn`, `evaluate_game`, `build_move_records`; Task 12 / 11 `fit_beta_binomial`, `beta_quantile`, `quantile`; Task 15 `kpi_catalogue`; Task 17 storage.
- Produces: `ReferenceGameDeps { pool, analyser, cache: Arc<dyn EvalCache>, enrich: EnrichDeps }`; `handle_reference_game(deps, job, payload, cancel) -> Result<(), JobError>` (full per-game pipeline for **both** colours, two `reference_observations` rows; when no `pending`/`analysing` games remain for the build, calls `finalize_band_reference`); `ReferenceGameHandler`; `ReferenceSide { player_hash, band, tc, player_rating, opponent_rating, metrics: PlayerGameMetrics, ctx: GameContext }`; `aggregate_band_reference(sides: &[ReferenceSide]) -> Vec<BandReferenceRow>` (TS M2 Task 14 rules: binomial pooled rate, Beta-binomial fit → p25/p40/median/p75, `epl_per_event`, `opps_per_game`; continuous KPIs from per-player means with type-7 quantiles; `opp_slope_per_100` by OLS; `low_sample = n_players < 10`); `finalize_band_reference(pool, build_id) -> Result<()>` (writes rows, records the difficulty model on the build, `activate_build`).

- [ ] **Step 1: Write the failing tests** (TS M2 Task 14): pure aggregate 10 players W1 (6, 300) + 10 players (12, 300) over 10 games → `pooled_rate ≈ 0.03` (6 dp), `p25 < 0.03 < p75`, `n_players 20`, `low_sample false`; 5 players → `low_sample true`; slope with value `1 + 0.5·(opp − own)/100` → `≈ 0.5` (6 dp); DB: 10 reference games (from Task 21's fixture selection) analysed with FakeAnalyser and memory stores → 20 observations, finalize writes `weakness:W1` rows and activates the build; `load_band_reference` returns it; a second build supersedes it; `reference_rows_keep_the_difficulty_model`; `finalize_runs_once` (two workers finishing the last two games concurrently → one `active` build, guarded by `pg_advisory_xact_lock(hashtext(build_id))`).
- [ ] **Step 2: Run** `$RUST cargo test -p profile --test band_reference`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Append to ADR-R2-11** (Beta-binomial fit instead of raw per-player rates, the opponent slope, comparability only within the same protocol, enrichment and difficulty model, provisional chess.com offsets). Commit `feat(profile): band reference aggregation from reference games`.

### Task 33 (I): `build-profile` job and events

**Files:**
- Create: `backend/crates/profile/src/{build_profile.rs,estimates.rs,timeline_job.rs,snapshots.rs}`, `backend/crates/profile/tests/common/seed.rs` (reused by Task 34)
- Modify: `backend/crates/events/src/store.rs` (add `UserEventType::ProfileUpdated = "profile.updated"`, `WeaknessChanged = "weakness.changed"`)
- Test: `backend/crates/profile/tests/build_profile.rs`

**Interfaces:**
- Consumes: Tasks 12, 14, 15, 17 (`load_band_reference`), 18 (`rebuild_repertoire`), 29 (`KIND_SCHEDULE`), 31 (loaders); R1 T16 `publish`.
- Produces: `BuildProfileDeps { pool, queue, now: fn() -> DateTime<Utc> }`; `handle_build_profile(deps, payload: BuildProfilePayload) -> Result<BuildProfileSummary { estimates, snapshots, transitions }, JobError>`; `BuildProfileHandler`. Steps exactly TS M2 Task 25 (record `started_at`; load games, series and reference with band per R-4; `estimate_weakness` per binomial key and upsert; `evaluate_series` per series, append history rows whose `(kind, to, at_game_id)` are new, update state and `p_imp`; KPIs for `last50` and every ISO week containing a game analysed since the last snapshot, appended only when `value` or `n` changed; area scores as `area:<area>` snapshots; `rebuild_repertoire`; publish after commit one `profile.updated {timeClass, area, kpis: [{metric, value, n, ciLow, ciHigh}]}` per area whose rows changed and one `weakness.changed {id, timeClass, state, fromState, onsetAt}` per new transition; re-enqueue itself if any game has `analysed_at > started_at`; enqueue `KIND_SCHEDULE`). Records a `job_metrics` row (`kind "build-profile"`, stages `load_ms`, `estimates_ms`, `timeline_ms`, `kpi_ms`, `repertoire_ms`, `persist_ms`).

- [ ] **Step 1: Write the failing tests** (seed: one account, 40 analysed blitz games with `game_metrics` and series points, W1 at 0.04 vs a reference of 0.02, one active reference build), TS M2 Task 25 cases: one estimate row per binomial key, W1 `confidence > 0.7` and state `active`; `kpi_snapshot` rows for `last50` and the 2 seed weeks; 6 `profile.updated` events (mindset has no benchmark) and a `weakness.changed` for W1; a second identical run appends 0 snapshot and 0 history rows and publishes no `weakness.changed`; a game with `analysed_at` after `started_at` (fake clock) → exactly one re-enqueue; no reference build → `NoBenchmark` estimates, all states `not_an_issue`, no error; `records_build_profile_stage_metrics`.
- [ ] **Step 2: Run** `$RUST cargo test -p profile --test build_profile`. Expected: FAIL. Implement. Rerun. Expected: PASS. Commit `feat(profile): build-profile job with snapshots, timeline and events`.

### Task 34 (I): `CoachingInput` builder with contract validation

**Files:**
- Create: `backend/crates/profile/src/coaching/{mod.rs,builder.rs,exemplars.rs,strengths.rs,mapping.rs}`, `docs/decisions/ADR-R2-18-build-profile-and-coaching-input.md`
- Test: `backend/crates/profile/tests/coaching_input.rs`

**Interfaces:**
- Consumes: Task 2 (`CoachingInput`, `validate`, `LIMITS`), Task 33 tables and seed, Task 26 `select_for_analysis` (for `backfill_complete`), Task 15 `kpi_catalogue`.
- Produces: `BuildError { Db(sqlx::Error), Contract(Vec<Issue>) }`; `build_coaching_input(pool, account_id: Uuid, tc: TimeClass, now: DateTime<Utc>) -> Result<CoachingInput, BuildError>`; `to_exemplar_position(row: &ExemplarRow) -> ExemplarPosition` (White-POV scores, SAN via `Position::play_uci`, PVs cut to 16, tags = `contract_tag()` of best and refutation tags, at most 12, `best: None` when played = best); `StrengthCandidate { area, label, kind, metric_key: Option<MetricId>, band_percentile: Option<f64>, n, exemplars }` and `select_strengths(…) -> Vec<coaching_input::Strength>`. Field rules: TS M2 Task 26 (time control = most common `(base, inc)`, `None` for daily; basis counts and `backfill_complete`; all 7 area scores; metrics from the latest `last50` snapshots; weaknesses with ranks, up to 5 exemplars for ranks 1–3 and 2 for the rest, from `exemplar_plies` within 6 months by ΔW then recency, distinct games; openings with non-null verdicts, up to 3 examples; changes from the last 90 days with an onset, causes and up to 3 examples per side; strengths at most 4, by percentile, each with ≥ 1 matching exemplar) plus Rulings R-1 to R-8. `validate()` runs last; a failure returns `BuildError::Contract`.

- [ ] **Step 1: Write the failing tests** (Task 33 seed with real moves), TS M2 Task 26 cases: areas equal `AREAS` in order; priorities non-increasing, ranks from 1; W1 has 3 exemplars; every exemplar's `best_pv[0] == best.uci` when `best` is `Some`, and replaying `played.uci` from `fen` gives `played.san`; every `refutation_pv` replays legally after `played`; basis `{analysed 40, imported 40, …}`; no `focus` key in the JSON; every strength has 1–3 exemplars; JSON round-trips unchanged. Contract tests:
  - `builder_output_validates_against_ts_schema` (`jsonschema` against `backend/fixtures/coaching-input/ts-schema.json`, and `validate()` is `Ok`);
  - `coaching_input_for_account_with_0_analysed_games_validates` (weaknesses, changes, strengths empty; 7 area scores `sufficient false`; `band == B2` with no ratings (R-4); `window_start == window_end == now` date (R-5); valid against the TS schema);
  - `builder_respects_limits_and_unit_ranges` (seed 31 opening lines with verdicts, a 20-ply PV, a 90-char opening name, `eval_trend_15_35 = −8`, a rate CI high of 1.04 → 30 openings, PV of 16, a line key ≤ 64, `eval_trend_15_35` absent from `metrics`, `high == 1.0`; the result validates);
  - `strength_has_exactly_contract_fields` (serialised keys are `area, label, metricKey, exemplars`).
- [ ] **Step 2: Run** `$RUST cargo test -p profile --test coaching_input`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Write ADR-R2-18** (the build-profile flow, event payloads, append-only rules, the `CoachingInput` contract and its mapping to M3 including R-1…R-8, the R3 hand-off: R3 depends on `coaching-input` only). Commit `feat(profile): coaching input builder validated against the m3 contract`.

### Task 35 (I): Worker wiring, CLI, runtime image, docs and live smoke

**Files:**
- Modify: `backend/crates/worker/src/{config.rs,start.rs}`, `backend/crates/cli/src/{lib.rs,main.rs}` (+ create `backend/crates/cli/src/profile_cmd.rs`, `queue_cmd.rs`), `backend/Dockerfile` (builder builds `-p lichess-db` too; runtime copies it), `docker-compose.yml` (`rust-worker`: `DATA_DIR=/data`, `SYZYGY_PATH=/data/syzygy`, `MAIA_URL`, volume `./data:/data:ro`), `CLAUDE.md` (R2 lines), `docs/decisions/README.md` (final titles), `docs/superpowers/plans/2026-11-21-roadmap.md` (R2 row)
- Test: `backend/crates/worker/tests/profile_stack.rs`, `backend/crates/cli/tests/profile.rs`

**Interfaces:**
- Consumes: everything above; R1 T22 `start_workers`, T23 CLI.
- Produces:
  - `WorkerEnv` gains `data_dir: Option<PathBuf>`, `syzygy_path: Option<PathBuf>`, `maia_url: Option<String>`, `plan_default: Plan` (`WORKER_DEFAULT_PLAN`, default `free`);
  - `start_workers` additionally: optional `SyzygyPool` (+ `CachedTbProbe`), `select_difficulty_provider`, `DbEnrichmentStore`, `ProfileHook` on the analyse handler, `FairRoundRobin` as the analyse runner's strategy, runners for `KIND_PREVIEW` (concurrency `max(1, pool/2)`), `KIND_SCHEDULE` (2), `KIND_BUILD_PROFILE` (2), `KIND_REFERENCE_GAME` (sharing the analyse pool; fair order puts it last), and `requeue_unenriched` once at start;
  - CLI: `cli profile --platform <p> --username <u> --time-class <tc> [--json]` printing `band B2  analysed 214/3912  focus-ready weaknesses N  areas: openings 54 …` (or the whole input as JSON); `cli queue-lag [--json]` printing `QueueLag` and `scale_advice`; `cli eta --platform <p> --username <u>` printing `format_eta`; `cli analyse` now runs import → schedule and waits until the selected games settle.

- [ ] **Step 1: Write the failing tests:** `start_workers_boots_without_syzygy_or_maia` (smoke, both unset); `start_workers_uses_fair_strategy_for_analyse`; `cli profile --json` on the Task 33 seed prints JSON that validates against the TS schema; `cli queue-lag --json` has keys `oldestP0AgeSeconds`, `queuedGames`, `coreSecondsQueued`, `advice`; R1's CLI tests still pass with the schedule step (`cli analyse` summary line unchanged).
- [ ] **Step 2: Run** `LANE=r2-i $RUST cargo test -p worker -p cli`. Expected: FAIL. Implement. Rerun. Expected: PASS.
- [ ] **Step 3: Live smoke** (network; not a test). Rebuild `rust-worker`, then:
  1. `docker compose run --rm --entrypoint cli rust-worker analyse --platform lichess --username DrNykterstein --max 40`
  2. `docker compose run --rm --entrypoint cli rust-worker profile --platform lichess --username DrNykterstein --time-class blitz --json > data/smoke-ci.json`
  3. `jq '.basis.analysedGames, (.areaScores|length), (.weaknesses|length)' data/smoke-ci.json`
  Expected: analysed > 0, area scores 7, exit 0. Areas say "no benchmark" until a band-reference build has run; that is acceptable and stated in the output. Validate the file with `$RUST cargo run -p coaching-input --example validate -- /work/data/smoke-ci.json` (an example binary added in this step; expected `valid`).
- [ ] **Step 4: Run** the full gate `$RUST sh -c "cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo deny check && cargo test --workspace"` and the schema drift script. Expected: PASS, no drift.
- [ ] **Step 5: Docs.** `CLAUDE.md`: one line each for `data/`, `lichess-db build|bench|syzygy`, `cli profile|eta|queue-lag`. ADR index: final titles for ADR-R2-1…18 (the controller renumbers). Roadmap: R2 done. Commit `feat(worker): wire profile engine, fair scheduler and cli`.

---

## Port checklist (TS M2 task → R2 task)

| TS M2 | R2 | Note |
|---|---|---|
| 1 contracts | 1 (types), 2 (contract crate) | coaching input split into its own crate |
| 2 schema | 3 | plus `game_previews`, `job_metrics.stages`, fairness index, `unverified` status |
| 3 board | 4 | plus `Position::play_uci` |
| 4 motifs | 5 | |
| 5 features | 6 | |
| 6 episodes | 7 | |
| 7 detectors | 8 | |
| 8 game metrics | 9 | `PlayerGameMetrics` |
| 9 strengths | 10, 34 | contract shape R-1 |
| 10 lichess db | 19 | streamed, never stored |
| 11 explorer | 17 (storage), 20 (build) | parallel pipeline + benchmark |
| 12 classification | 16 | `include_str!` dataset |
| 13 repertoire | 18 | |
| 14 band reference | 21 (selection), 32 (analysis, aggregation) | single-pass bottom-k |
| 15 numerics | 11 | |
| 16 posteriors | 12 | |
| 17 detectors | 13 | |
| 18 timeline | 14 | |
| 19 KPI | 15 | contract units |
| 20 Syzygy | 22 (fetch), 23 (probe) | |
| 21 Maia | 24 | |
| 22 enrichment | 25 | |
| 23 scheduler | 26, 29 | plus fair dequeue (27), preview (28), ETA/lag (30) |
| 24 analysis features | 31 | hook trait in `analysis` |
| 25 build-profile | 33, 35 | |
| 26 coaching input, CLI | 34, 35 | TS-schema parity |

## Self-review

1. **Spec coverage.**

| Requirement | Task |
|---|---|
| Motif tagger (§8.3, improvement-engine §4.4, own rules) | 5 |
| Threat probe (§4.4) | 4, 6, 25 |
| MultiPV on critical positions (§8.1) | 25 |
| Episodes W5, W6, W9a | 7 |
| Detectors W1–W12, per-opportunity metrics (§6.2) | 8 |
| Beta posteriors, priors, P_imp, priority (§5.2–5.4, §6.6) | 12 |
| Band references (§8.6) | 21, 32 |
| Own explorer (§14) | 17, 20 |
| Repertoire, struggles, verdicts (§8a) | 16, 18 |
| Syzygy (§14) | 22, 23 |
| Maia plus fallback, licence first ("8. (cont.)") | 24 |
| Area scores (§8b) | 15 |
| `game_metrics` / `kpi_snapshot`, Wilson, bootstrap, opponent adjustment (§12a) | 9, 15, 33 |
| Timeline: BOCPD, CUSUM, states, onset, causes, tables (§12b) | 13, 14, 33 |
| Tiers P0–P3, adaptive stopping, stratified sample, `imported` (§8c) | 26, 29 |
| Fair round-robin slices of 5 by tier then join time (§8d.1) | 27 |
| Preview pass at 30k nodes, never cached or used for stats (§8d.2) | 28, 31 |
| Autoscaling hooks: P0 lag, core-seconds, caps (§8d.4) | 30, 35 |
| Back-pressure ETA, never a spinner (§8d.5) | 30, 35 |
| build-profile and events after commit (§13a) | 31, 33 |
| `CoachingInput` = M3 contract, schemars, TS-schema parity | 2, 34 |
| Lichess dump: streamed HTTP, zstd, SHA-256, rayon parse, sampling, benchmark | 19, 20, 21 |
| Per-stage timing metrics | 3, 20, 25, 28, 31, 33 |
| Strength exemplars (controller) | 10, 34 |

   Gaps left on purpose, as in TS M2: playstyle axes and style-clash `replace` (ADR-R2-8 defers them); fatigue and plan-based P_imp (M6/M7); EWMA drift on plans (M7); the rating-conditioned T_win fit (§4.3); pooled-user benchmarks replacing Lichess references; pre-seeding the eval cache from `lichess_db_eval.jsonl.zst` (§8.1, not in the M2 roadmap row); an actual cloud autoscaler (only its advice function is built).
2. **Step scan.** Test steps name their tests and values (TS M2 values restated or, where long, named by TS M2 task with the instruction that they are binding). Code steps give signatures, files and pinned constants. Bodies appear only for the settled swing, BOCPD (by reference to TS M2's pseudocode), `next_state` (TS M2's rule list), the fair dequeue SQL and the bottom-k selection. No "TBD" lines.
3. **Type consistency.** `PlayerGameMetrics` (T1) is used in T9, T15, T31–T33; `RawMoveFeatures`/`MoveFeatures`/`EnrichedMove` (T1) in T6, T25, T31; `BandReferenceStat`/`BandReference` (T1) in T12, T15, T17, T32; `SelectStrategy::dequeue(queue, kinds, owner, limit, lease)` (T27) in T35; `job_priority`/`fair_tier` (T26) in T27, T29, T30; `ExplorerMove` (T17) in T18, T31; `enrich_game` → `Enriched.stages` (T25) in T31; `AnalyseHook`/`PreparedHook` (T31) only in T31/T35; `KIND_*` and payloads (T1) in T21, T28, T29, T31, T33, T35; `coaching_input::{AreaScore, CandidateCause}` produced directly by T15 and T14 and consumed by T34; `StrengthCandidate` (T34) vs contract `Strength` (T2) per R-1.
4. **Review Focus.** Five lines, each with a named test in its owning task (T25; T34, T12, T15; T34, T2; T20; T8, T15). The four further failure modes are listed with their tests.
5. **Proportion.** The plan converts 26 TS tasks plus five new subsystems (contract parity, streaming pipeline with benchmark, fair dequeue, preview, ETA/lag) into 35 tasks. It restates decisions and test values and points at TS M2 only for long rule tables that are already normative, instead of transcribing them.

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-11-21-R2-profile-engine.md`. The owner asked for maximum parallel agents, so execute it **subagent-driven, lane by lane**: one worktree per lane, a fresh implementer and reviewer per task, waves as in the tables above (starting alongside R1 as each lane's R1 prerequisites merge), and a whole-branch review after W6 and again after Task 35. Before execution the controller renumbers `ADR-R2-1…18`. Please review the plan and confirm it captures what you want.
