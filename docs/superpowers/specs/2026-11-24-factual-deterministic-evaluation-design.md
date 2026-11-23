# Factual and deterministic: the evaluation programme

Written 2026-11-23, for the roadmap from v0.2 to v0.5. Decision: ADR 0104. Plan: the owner-approved plan "a factual, deterministic Rematch" (four layers, the determinism contract, Phases 1 to 4).

Every recommendation Rematch makes will be a fact derived by one versioned, deterministic algorithm, and every word it shows will be checked against those facts. This spec says what that means layer by layer, how each guarantee is tested, what each layer is measured against, and which numbers we publish. It builds on the Tier 1 benchmark (ADR 0100, `docs/benchmark.md`), the ranking floor (ADR 0101), the coaching-input snapshot (ADR 0086), the validator (ADRs 0093 and 0102) and the plan context (ADR 0094).

## 1. Four layers

| Layer | What it produces | Guarantee | Ground truth |
|---|---|---|---|
| 1. Facts | Engine evaluations, win percentages, accuracy, judgements, tablebase results, motif tags, per-move features, weakness events and chances | Deterministic: the same game and the same algorithm version give byte-identical facts | **Stockfish 19** itself (pinned build and settings, section 2); **Syzygy** 3-4-5 tables (exact for 5 pieces or fewer); the **Lichess** puzzle database's theme labels (CC0) for the motif tagger; the accuracy reference cases, checked against an independent re-implementation of lila's formula (`backend/fixtures/accuracy/lila-examples.json`); injected flaws for the detectors |
| 2. Profile | Rates per chance with 80% ranges, comparisons with the peers, the trajectory, pattern series | Deterministic: seeded bootstraps, canonical ordering, no wall clock in the maths | Synthetic players with known true rates (Tier 1); calibration (do the 80% ranges hold the truth about 80% of the time?) |
| 3. Recommendation | The ranked weaknesses, the weekly focus and its target, the drill set, the plan's order | Deterministic and computed, with no language model involved | Tier 1 (is the injected flaw ranked first?); Tier 2, predictive validity on anonymised Lichess players; Tier 4, the trainer's held-out positions (ADR 0090) |
| 4. Language | Report text, coach answers, weekly reviews (Claude) | Never a source of facts: it verbalises layer 1 to 3 facts through citation tokens, and the validator drops any uncited number, claim or engine statement (ADRs 0060, 0093, 0102) | Deterministic checks on every output: citation coverage, the drop rate, the recommendation named in the text equals the computed one, no fact outside the input |

Layer 4 is not deterministic and does not need to be. Its non-determinism is contained: it can change how a recommendation is worded, never which recommendation is made. An LLM judge may score layer 4 as an advisory signal; it is never a gate, because the same text can get two scores.

Two rules run across all four:

- No silent change. A change to a constant, threshold, detector, protocol, prompt, model or rule is a new algorithm version, recorded in `algorithm/CHANGELOG.md` and benchmarked before release (section 3).
- Show your source. Every number and recommendation in the app can show the facts behind it and the algorithm version that produced it ("Why this?", section 9).

## 2. The determinism contract

### What must be byte-identical

Given the same input games and the same `algorithm_id`, two runs produce the same bytes for:

1. Facts: `position_evals` (score, best move, PV), `position_multipv`, `position_threats`, `position_tb`, `move_features`, `game_metrics.metrics`, `opponent_game_metrics`, `weakness_series_point`.
2. Profile: `weakness_estimates`, `weakness_state_history`, `kpi_snapshot`, `account_benchmark_stat`.
3. Recommendation: the CoachingInput JSON in canonical form (sorted keys, the `coaching-input` crate's serialisation), its `input_hash` (ADR 0086), the drill set from `ai::output::drills::drills_for`, and the focus target.

Wall-clock columns are not facts and are excluded from the comparison: `created_at`, `updated_at`, `computed_at`, `validated_at`, the CoachingInput's `generatedAt`, and every `*_ms` timing. Row ids (`bigserial`, `gen_random_uuid()`) are excluded too; rows are compared by their natural keys (`fen_key` and `protocol`, game and ply, account, time class and series key).

### How it is tested

The replay suite (v0.2) runs the pipeline twice, in two throwaway databases, on fixed real games:

- `backend/fixtures/pgn/*.pgn` (including `terminal/`, `chess960.pgn` and `no-clocks.pgn`);
- `backend/fixtures/lichess-db/sample.pgn` (5 CC0 Lichess games) and `backend/fixtures/lichess-games.ndjson`;
- `backend/fixtures/chesscom-month.json` (the importer's chess.com shape).

It dumps the tables above in canonical form (natural-key order, the excluded columns removed, floats as their shortest round-trip decimal) and compares the two runs with each other and with golden snapshots committed under `backend/crates/algo/tests/golden/`. A diff fails CI unless the same change bumps the algorithm version and regenerates the goldens. Tier 1 already proves one slice of this: `backend/crates/bench/tests/pipeline.rs` generates a game twice and compares them, and score (e) builds the CoachingInput twice from the same rows (`docs/benchmark.md`).

The suite also runs on two machines, the developer's and the CI runner: same goldens, same bytes. **Stockfish** with one thread and a fixed node count is deterministic across x86-64 machines for the same binary and the same hash size; the suite is how we find out if that ever stops holding.

### The engine settings

| Setting | Value | Where it is set today |
|---|---|---|
| Build | **Stockfish 19**, `stockfish-linux-x86-64-universal.tar.gz`, sha256 `9defc0d4e55d49c65a6d042f3e571a39fcea499ade6dbe741b53b8c65e03611f` | `backend/Dockerfile` (`STOCKFISH_ASSET`, `STOCKFISH_SHA256`), the same in `apps/worker/Dockerfile` |
| Main analysis | 300,000 nodes, MultiPV 1, protocol `sf19-n300k-v1` | `ENGINE_NODES` and `ANALYSIS_PROTOCOL`, `backend/crates/engine/src/types.rs` |
| Enrichment | `enr-v1`; MultiPV 3 at 300,000 nodes (`sf19-n300k-mpv3-v1`) on at most 30 critical plies; threats at 50,000 nodes on the null-move position (`sf19-n50k-null-v1`) | `backend/crates/enrich/src/protocol.rs` |
| New game | `ucinewgame` before every search, then `position fen`, then `isready` as a barrier | `UciEngine::analyse`, `backend/crates/engine/src/uci.rs` |
| Search limit | `go nodes N`, never a time limit | the same function |
| Threads | 1 | Not sent. The pool starts engines with `threads: None` (`backend/crates/engine/src/pool.rs`), so **Stockfish**'s default of 1 applies |
| Hash | 16 MB | Not sent by the worker (`hash_mb: None`, `backend/crates/worker/src/start.rs`), so the default applies; the bench sends `Some(16)` (`backend/crates/bench/src/run.rs`) |
| Tablebases | The pinned 3-4-5 set, 145 WDL and 145 DTZ files | `backend/crates/lichess-db/data/syzygy-345.sha256`, checked by `lichess-db syzygy` |

Threads and Hash are deterministic today only because Stockfish's defaults happen to be 1 and 16. v0.2 makes both explicit (`threads: Some(1)`, `hash_mb: Some(16)` in every `PoolOptions` and `StartOptions`) and adds an audit test that records the UCI lines a pooled engine receives and asserts `setoption name Threads value 1`, `setoption name Hash value 16`, a `ucinewgame` before each `position`, and `go nodes` with the protocol's count. Two other protocols are outside the algorithm and stay out of the manifest: `PREVIEW_PROTOCOL` (`sf19-n30k-preview-v1`, the quick preview, never stored as a fact) and `INTERACTIVE_PROTOCOL` (`sf19-n1m-mpv3-interactive-v1`, the coach's own analysis tool, whose results the validator treats as cited evidence of that turn only).

### Randomness and the clock

- Every random draw in layers 1 to 3 comes from mulberry32 with a seed derived from the data: `profile_core::stats::rng` in Rust and `mulberry32` with `dataSeed` in `apps/web/src/server/stats/bootstrap.ts` (seeded since 2026-11-23, so the same games always give the same 80% range). The two are bit-identical by design.
- The ban: no `Math.random`, no `Date.now()` and no `new Date()` without an argument in the TS maths modules (`apps/web/src/server/stats`, `server/train/model.ts`, `server/focus/goal.ts`, `server/overview`, `server/profile`), and no `rand::`, `thread_rng`, `SystemTime::now` or `OffsetDateTime::now_utc` in `profile-core`, `enrich`, `coaching-input` or the profile builder's maths. Time enters as a parameter: `build-profile` already passes the build's clock as `now` (ADR 0086), and the trainer's due dates take today's date as an argument.
- The test that enforces it generalises the existing one, `it("has no Math.random anywhere in server/stats")` in `apps/web/test/stats/bootstrap.test.ts`, to the full TS list above, and adds a Rust test in the `algo` crate that scans the listed crates' `src/` for the banned calls. An allow-list names the few legitimate uses (the job scheduler, the database's `now()`), each with a reason.

### The language layer

Determinism ends at layer 4 on purpose. What stays deterministic is the check: the recommendation a report or a chat answer names (its focus key, the top weakness it leads with, the drill it suggests) must equal the computed one in the CoachingInput and the plan context. The check reads the text's citation tokens, never its prose, so it is exact.

## 3. The algorithm manifest

### What it names

The manifest is one JSON file, `algorithm/manifest.json`, generated from the code. Each field maps to a constant that exists today, or to one v0.2 adds:

| Field | Value today | Source |
|---|---|---|
| `facts.engine_protocol` | `sf19-n300k-v1` | `ANALYSIS_PROTOCOL`, `backend/crates/engine/src/types.rs` |
| `facts.engine_nodes` | 300000 | `ENGINE_NODES`, same file |
| `facts.stockfish` | release `sf_19`, asset and sha256 | `backend/Dockerfile` |
| `facts.engine_options` | Threads 1, Hash 16, `ucinewgame` per search | `engine/src/pool.rs`, `uci.rs` (made explicit in v0.2) |
| `facts.enrichment_protocol` | `enr-v1`, `sf19-n300k-mpv3-v1`, `sf19-n50k-null-v1`, `MAX_CRITICAL_PLIES` 30 | `backend/crates/enrich/src/protocol.rs` |
| `facts.syzygy` | sha256 of the pinned manifest | `backend/crates/lichess-db/data/syzygy-345.sha256` |
| `facts.maia` | `maia2==0.11.0`, model `rapid` (also `blitz`), Elo clamp 1000 to 2100 | `services/maia/pyproject.toml`, `services/maia/Dockerfile` (`MAIA_MODEL`), `services/maia/app/model.py` |
| `facts.player_game_metrics_version` | 3 | `PLAYER_GAME_METRICS_VERSION`, `backend/crates/profile-core/src/types.rs` |
| `facts.phase_metrics_version` | 3 | `PHASE_METRICS_VERSION`, `apps/web/src/server/stats/game-stats.ts` |
| `profile.detector_version` | `tl-v1` | `DETECTOR_VERSION`, `backend/crates/profile-core/src/timeline/states.rs` |
| `profile.source_digest` | sha256 of the listed files | `profile-core/src/constants.rs` (n0, trainability, bands), `weakness/detectors.rs` (`DECISIVE_DW` 15), `weakness/observe_game.rs` (`T_WIN` 85), `stats/`, `bands.rs`, `profile/src/peer_benchmark.rs` (`PEER_RATING_WINDOW` 150) |
| `recommendation.ranking_version` | `rank-v1` (new) | names the ADR 0101 rule: `RANK_MIN_EXCESS_PER_GAME` 0.02, `MODEL_LOSS_PER_WINNING_EPISODE` 0.15, `MIN_GAMES` 30, in `backend/crates/profile-core/src/stats/priority.rs` |
| `recommendation.coaching_input_version` | `coaching-input-v3`, `weekly-input-v1` | `INPUT_VERSION`, `backend/crates/coaching-input/src/limits.rs`; `WEEKLY_INPUT_VERSION`, `backend/crates/profile/src/coaching/weekly.rs` |
| `recommendation.trainer_version` | `trainer-v1` (new) | a new `TRAINER_VERSION` in `apps/web/src/server/train/model.ts`, over `INTERVALS`, `HOLDOUT_PERCENT` 20, `MIN_CHANCES` 30, `SESSION_SIZE` 12, `DAILY_CAP` 24, `NEW_PER_DAY` 10 |
| `recommendation.focus_version` | `focus-v1` (new) | a new `FOCUS_VERSION` in `apps/web/src/server/focus/goal.ts`, over `DEFAULT_GAMES_N` 20, `RELATIVE_STEP` 0.25, `MIN_PEER_STEP` 0.1, `MIN_VERDICT_CHANCES` 10 |
| `recommendation.drills_digest` | sha256 | `backend/crates/ai/src/output/drills.rs` (`drills_for`) |
| `language.prompt_sets` | one sha256 per set | `prompt_set_hash` over `REPORT_PROMPT_SET`, `CHAT_PROMPT_SET`, `WEEKLY_PROMPT_SET`, `CHANGE_PROMPT_SET`, `CHAT_SUMMARY_PROMPT_SET`, `backend/crates/ai/src/prompts.rs` |
| `language.models` | `claude-sonnet-5-5`, `claude-haiku-5-5`; API version `2026-10-21` | `AiModel`, `backend/crates/ai/src/client/model.rs`; `ANTHROPIC_VERSION`, `client/wire.rs` |
| `language.validator_digest` | sha256 | `backend/crates/ai/src/validator/` |

`TRAINER_VERSION`, `FOCUS_VERSION` and `RANKING_VERSION` do not exist yet; v0.2 adds them. A `source_digest` covers constants that have no version name of their own: change one number in `constants.rs` and the digest moves.

### The id

`algorithm_id` is a readable version and a content hash: `algo-<major>.<minor>.<patch>+<hash>`, where `<hash>` is the first 12 hex characters of the sha256 of the manifest's canonical JSON (sorted keys, no whitespace), for example `algo-1.0.0+3f9c2a1b7d4e`. The version is for people; the hash is the proof.

### The version policy

- Major: layer 1 to 3 output can change (a protocol, a constant, a detector, the ranking, the trainer or focus rules). The replay goldens change, and the Tier 1 scorecard is rerun.
- Minor: only layer 4 changes (a prompt file, a model id, the validator). The goldens are unchanged; the language-layer checks are rerun with `--live`.
- Patch: no output changes (a refactor or a comment inside a digested file). The goldens must be byte-identical, which is what proves it.

### The changelog

`algorithm/CHANGELOG.md` has one entry per version, newest first: the id, the date, what changed, why, the ADR, and the benchmark delta (the guarded KPIs before and after, from `bench compare`, section 6). A version without an entry does not merge.

### The CI guard

1. A Rust test in the new `algo` crate builds the manifest from the constants it imports (`engine`, `enrich`, `profile-core`, `coaching-input`, `ai`) and the files it digests, and fails when it differs from the committed `algorithm/manifest.json`. A web test does the same for the TS fields.
2. `scripts/algorithm/check.mjs`, in the `ts` CI job, compares the manifest with the merge base's. When any component changed, the version must have changed by the policy above and `algorithm/CHANGELOG.md` must have an entry for the new id. A patch bump is allowed only when the replay goldens are unchanged.
3. `cargo run -p cli -- algo manifest --write` regenerates the file. The test and the check never write.

The writers do not depend on the `algo` crate (that would make a cycle with `ai` and `profile`). The id lives in `common::algorithm`, which reads `algorithm/manifest.json` with `include_str!`. `backend/Dockerfile.dockerignore` gains `!algorithm/`, and both build stages copy `algorithm/` next to `backend/` and `db/`.

## 4. Stamping

One migration adds `algorithm_versions (id text PRIMARY KEY, manifest jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT now())`, filled by the worker at start-up with its own manifest (insert, do nothing on conflict), and a nullable `algorithm_id text REFERENCES algorithm_versions (id)` on:

- the language layer and its inputs: `coaching_inputs`, `report_runs`, `weekly_reviews`, `coach_messages`;
- the plan: `focuses`, `drill_items`;
- the profile: `kpi_snapshot`, `weakness_estimates`, `weakness_state_history`;
- the facts: `game_metrics`.

`position_evals` gets no column: its key is (`fen_key`, `protocol`), and `protocol` is the manifest's `engine_protocol`, so the eval cache is shared by every algorithm version with the same engine protocol, as it should be. Existing rows keep `NULL`, which means "before versioning". Every writer fills the column from `common::algorithm::id()`; a DB test per writer asserts it. The web writers (`focuses`, `drill_items`) read the id from the same JSON file.

## 5. The evaluation lab

### Run records

Every benchmark run writes one RunRecord, an extension of `bench-results.json` (`RunOutput`, `backend/crates/bench/src/run.rs`), which today holds the scorecard, the suite, the engine searches, the cache hits, the seconds and every player's results. The RunRecord adds:

- `algorithm_id` and the full manifest;
- the commit, and whether the tree was clean;
- the suite name, its seeds, and the checksums of its data snapshots;
- the machine: CPU model, cores, RAM, OS and kernel;
- the Stockfish banner (`engine::version::stockfish_version`) and the binary's sha256;
- per-stage durations and the engine count;
- for `--live` runs, the token counts and `cost_micro_usd` per call (section 7).

A JSON schema for it lives in `bench/run-record.schema.json`, and a test validates a fixture record against it.

### Ground-truth suites

Every suite is deterministic: fixed seeds, fixed data snapshots with checksums, no network in CI.

1. Motif tagger against Lichess puzzle themes. The **Lichess** puzzle database (`lichess_db_puzzle.csv.zst`, CC0) labels each puzzle with themes, assigned by Lichess's tagger and refined by players' votes, on millions of puzzles that players have solved. The tagger's tags are named after the same themes (`MotifTag`, `backend/crates/profile-core/src/types.rs`: `hangingPiece`, `fork`, `pin`, `skewer`, `discoveredAttack`, `deflection`, `sacrifice`, `promotion`, `mate`, `mateIn1` to `mateIn4Plus`, `backRankMate`, `smotheredMate`; `matingNet` has no Lichess theme and is not scored). The suite pins one snapshot by sha256, samples a fixed subset of about 2,000 puzzles per theme with a seeded draw, plays the opponent's first move, runs `tag_line` on the solution line, and reports precision and recall per motif with 80% Wilson ranges. Only the data is used: the Lichess tagger `cook.py` is AGPL and stays out of the code, as ADR-R2-3 requires. The sampled subset is committed (CC0 allows it); the full dump is never in the repository.
2. Tablebase agreement. Positions with 5 pieces or fewer from the replay fixtures and the puzzle sample, probed in the pinned Syzygy set: the share whose stored win, draw or loss judgement agrees with the tablebase's WDL, and the W8 detector's events checked against DTZ. Disagreements are listed by FEN.
3. Accuracy formula. `backend/fixtures/accuracy/lila-examples.json` holds game-accuracy cases from an independent audit, each with an independent re-implementation of lila's `AccuracyPercent.gameAccuracy`. `backend/crates/chess-core/tests/game_accuracy.rs` already tests them; the suite reports the largest difference as a KPI so a change shows on the scorecard, not only as a failing test.
4. Tier 1+, profile and recommendation. The Tier 1 harness (`backend/crates/bench`) gains injectors (`bench::flaws`) for missed forks, pins and skewers (W3 by motif), back-rank weaknesses, endgame technique (W8, which needs the tablebase), getting short of time and rushing important moves (W9a, W9c), leaving the opening worse (W7.prep), tilt after a loss (W12), and two or three flaws at once. Players are generated across the audience, rated 600 to 2000 in steps of 200, with opponents in the same band. The three open gaps are measured here: #11 (the W4.missed peer row is low-sample at 80 games), #13 (W1's 80% interval is pulled towards the opponents' rate, so coverage fell to 0.6) and #14 (early-trap blunders that hang a piece rank as W1, so W7.traps drops out for `bench-opening`).
5. Tier 2, predictive validity. A fixed, checksummed sample of anonymised players from one pinned Lichess month (`reference_games`, whose players are already hashed: `white_hash`, `black_hash`). For each player, the profile is built from games before a date T, and the games after T measure where points are lost. Scores: the Spearman correlation between the ranked weaknesses and points lost per key after T, and the lift of the top-ranked key over a baseline ranking by base rate. No real chess.com player's games are used.
6. The language layer. A fixed set of CoachingInputs (from the Tier 1 players and `backend/fixtures/coaching-input/valid-sample.json`) through the real report, chat and weekly-review paths, scored deterministically: citation coverage (the share of paragraphs with at least one resolved citation), the validator's drop rate by reason (`report_sections.dropped`, `coach_messages.dropped_count`), "the recommendation in the text equals the computed one", and the forbidden-claim checks (no number, move or engine word outside a citation). It needs `--live` and real tokens, so it never runs in CI; results are stored per prompt-set hash with their token cost.

### The regression gate

Each pull request that touches the pipeline runs the deterministic suites (1 to 4) against the reference algorithm on the same seeds and fails on a clear regression in a guarded KPI. "Clear" means the 80% interval of the difference lies wholly on the worse side, which is the rule the app already uses for "better or worse only when it's clear". `ALGO_VARIANT=<name>` runs a second algorithm version side by side in the same run (the same games, two profiles), so a ranking change can be compared without a second run's noise.

## 6. History and comparison

- Storage. RunRecords are committed to a data branch, `bench-history`, as `runs/<date>-<suite>-<algorithm_id>.json`. The branch is outside the `main` ruleset (`.github/settings/rulesets/main.json` protects `~DEFAULT_BRANCH` only), and only the weekly benchmark workflow and the release workflow write to it, with a token scoped to `contents: write` on that job alone. Nothing on `main` is rewritten by a bot.
- `cli bench compare A B` reads two RunRecords (files, or ids on the branch) and prints, per KPI, A, B, the difference with a bootstrap 80% interval over seeds and players, and a verdict: better, worse, or no clear difference. It exits 2 on a clear regression in a guarded KPI, which is what CI uses.
- The site reads the branch at build time (section 8).

## 7. The KPI catalogue

Every KPI has a definition, its source, and an 80% interval over seeds and players (Wilson for shares, a seeded bootstrap for means and percentiles).

### Quality

| KPI | Definition | Source |
|---|---|---|
| Motif precision and recall | per motif, tagged and labelled over tagged (precision), over labelled (recall) | puzzle suite |
| Tablebase agreement | share of positions with 5 pieces or fewer whose judgement equals Syzygy WDL | tablebase suite, `position_tb` |
| Accuracy difference | the largest absolute difference against the `lila` value | `lila-examples.json` |
| Recall at 1 and 2, top-1 precision, clean false alarms, plan hit rate, games to detection, top-2 overlap, determinism | as defined in `docs/benchmark.md` | `Metrics`, `backend/crates/bench/src/results.rs` |
| Calibration | share of the target's 80% intervals holding the true rate (well calibrated: about 0.8) | Tier 1, `weakness_estimates` |
| Predictive validity | Spearman correlation and the lift at rank 1 | Tier 2 |
| Citation coverage, drop rate, recommendation match, forbidden claims | as in section 5 | language suite, `report_sections.dropped`, `coach_messages.dropped_count` |

Guarded KPIs (a clear regression fails a pull request): recall at 2, top-1 precision, clean false alarms, calibration distance from 0.8, plan hit rate, top-2 overlap, determinism, motif recall per motif, tablebase agreement, and the accuracy difference.

### Cost

| KPI | Definition | Source |
|---|---|---|
| Engine CPU-seconds per game | `sum(engine_cpu_ms) / 1000` per `game_id`, `kind = 'analyse-game'` | `job_metrics` |
| Positions and cache hits per game | `positions`, `engine_positions`, and `cache_hits / positions` | `job_metrics` |
| Enrichment engine calls per game | `multipv_engine_calls + threat_engine_calls` | `job_metrics.stages` |
| AI cost per report, answer and weekly review | `sum(cost_micro_usd)` per `ref_id` (the row the job wrote, for example the report run), with `feature` `report`, `chat` or `weekly_review` | `ai_usage` (`cost_micro_usd` is ours, priced by `PricingTable`; `sdk_estimate_micro_usd` is kept for comparison only) |
| Modelled USD per active player per month | games a month times engine CPU-seconds per game times a stated CPU price, plus reports, answers and reviews a month times their cost; the prices and usage assumptions are fields of the RunRecord | the two rows above |

### Latency and throughput

| KPI | Definition | Source |
|---|---|---|
| Per-stage p50 and p95 | `queue_wait_ms`, `parse_ms`, `cache_ms`, `engine_wall_ms`, `persist_ms`, `total_ms`, and the `stages` keys (`multipv_ms`, `threat_ms`, `tb_ms`, `difficulty_ms`, `features_ms`, `load_ms`, `estimates_ms`, `timeline_ms`, `kpi_ms`, `repertoire_ms`, and the rest) | `job_metrics`, by `kind` |
| Import to analysed, per game | the first `job_metrics.created_at` of `analyse-game` for the game, minus `games.created_at` | `job_metrics`, `games` |
| Profile build | `total_ms`, `kind = 'build-profile'` | `job_metrics` |
| Report wall time | `finished_at - started_at` | `report_runs` |
| Coach first token and total | from the turn's request to its first streamed token, and to its last | not stored today: v0.3 adds `first_token_ms` and `total_ms` to `coach_messages` |
| Games per hour per core | analysed games over wall hours over engines | RunRecord |

The KPIs that read production tables are computed by `cli kpi --since <date> [--json]` (read-only, like `cli queue-lag`), so a self-hoster can run them on their own database. The benchmark reads the same queries from its throwaway database.

## 8. The Evaluation page

A new site page, `/docs/evaluation/`, generated at build time from the `bench-history` branch (with a committed fixture as the offline fallback, as the roadmap page does, ADR 0099):

- one chart per guarded KPI over time, coloured by algorithm version, with its 80% band;
- the latest scorecard, as `docs/benchmark.md` shows it today;
- the known gaps, each linked to its issue;
- the cost and latency tables;
- the algorithm changelog, from `algorithm/CHANGELOG.md`.

It follows `packages/ui/DESIGN.md` through the site's generated tokens. Charts are static SVG made at build time, so the page has no client JavaScript. `docs/evaluation.md` explains the method in words and links here.

## 9. "Why this?"

Every recommendation (the top weakness, the weekly focus and its target, a drill, a plan item) gets a "Why this?" disclosure that shows:

- the facts behind it: events and chances, the rate with its 80% range, the comparison and its group, the points lost per game;
- the rule that ranked it (the ADR 0101 floor, in words);
- the algorithm id, linked to its changelog entry.

v0.2 builds the data: one server function per recommendation that returns these facts from the stamped rows. v0.5 builds the disclosure in the UI round.

## 10. Patterns and progression (v0.4)

- The pattern registry. A pattern is one versioned (chance, event) rule: what counts as a chance, what counts as the event. The weakness keys (W1 to W12) and the motif miss rates are registered under it, and new patterns are admitted only after they pass their ground-truth suite. Planned families: by motif, by piece, by phase and endgame type, by time use, by opening family, after a loss, and by colour.
- The progression monitor. At every profile build, `kpi_snapshot` and each pattern's rate feed the change detection that `weakness_state_history` already runs (BOCPD with a Beta-binomial predictive and a CUSUM early warning, ADR-R2-6), generalised from weakness keys to patterns.
- On Progress: "Your patterns over time", with the trend, the 80% range, the onset and the recovery, and the before and after of a focus or of drills.
- In the coach: the weekly review and the chat receive confirmed changes as citable facts (coaching input v4), never unconfirmed ones.
- The operator dashboard, for the maintainer only: cohort trends per algorithm version, active players and cost per player. It feeds Tier 4 for players who opt in, anonymised.

## 11. The UI round (v0.5)

The round starts from real-screenshot mockups; the owner picks before anything is built.

- Sound: the trainer plays `correct`, `wrong` and `pen` (they exist in `packages/ui/src/audio/sounds.ts`, and only the game review uses them today, `review-view.tsx`), and game end; mute and volume in the header and on a new Settings page; the sound timed to the piece landing; a tuning session with the owner over three sound sets.
- The board: a promotion picker (the board promotes to a queen today, `packages/ui/src/board/board.tsx`: "a promotion picker arrives with the drill UI; queen for now"); the drill's right and wrong sequences; piece-set and board-theme settings within the design system; premoves; no overflow at 360 px; a bigger trainer board on phones.
- Information: the overview leads with the trajectory, one focus and a practise button, and the rest is one click away; one "Not judged yet" list; one number style everywhere; "Why this?" on every recommendation.
- The phone and screen-reader epic moves here from More players: the axe check and the touch targets on the real trainer page.

## 12. Non-goals

- Making the language model deterministic. Temperature and sampling are not the contract; the check is.
- An LLM judge as a gate. It stays advisory (Tier 3 in ADR 0100).
- Benchmarking on real chess.com players' games in CI or on the site. Tier 2 uses anonymised Lichess players; the owner's own account stays a manual check.
- Cross-architecture determinism (x86-64 against ARM). The pinned build is x86-64; an ARM build would be a new `facts.stockfish` and so a new major version.
- Re-analysing stored games when the version changes. Old rows keep their old id; a re-analysis is a separate, explicit operator action.

## 13. Risks and open questions

- Determinism across machines rests on Stockfish's node-limited search being reproducible for one binary and one hash size. We believe it is; the two-machine replay is the measurement.
- Digests over whole files make every comment edit a patch bump. That is the price of no silent change; the patch policy keeps it cheap (the goldens prove nothing moved).
- Lichess puzzle themes are labelled for the puzzle's solution, not for the position's every motif, so recall is the cleaner measure; low precision may mean an unlabelled true motif. Disagreements get a manual sample before a motif's precision is called a fault.
- Tier 2 measures prediction, not improvement. A weakness can predict lost points and still not be the best thing to practise; Tier 4 is the measurement for that.
- The `bench-history` branch grows by one file a week; at about 200 KB a run that is about 10 MB a year, which a git branch carries without trouble.
- Weekly runs on shared runners vary in speed by tens of percent, so latency KPIs on the site come from one named reference machine, and CI never gates on latency.
