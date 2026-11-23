# 0104. Factual and deterministic: four layers, one versioned recommendation algorithm

Date: 2026-11-23. Status: accepted (lane `evalspec`, branch `docs/evaluation-programme`, the owner-approved plan "a factual, deterministic Rematch"). Spec: `docs/superpowers/specs/2026-11-24-factual-deterministic-evaluation-design.md`. Builds on ADRs 0086, 0093, 0094, 0100, 0101 and 0102. Amends ADR 0099 (how `apply-settings.sh` treats milestones and seeded issues).

## Context

Rematch's facts come from **Stockfish** and our own statistics, and the coach only words them (the honesty rules, ADR 0060). Three things were still missing:

- Nothing named the algorithm. A recommendation depends on about thirty constants, protocols, prompt files and model ids spread over eight crates and the web app. A change to any of them (`RANK_MIN_EXCESS_PER_GAME`, a prompt version, a trainer interval) was visible only in the diff, and no stored row said which rules produced it.
- Determinism was true by accident in places. The engine pool sends neither `Threads` nor `Hash` (`backend/crates/engine/src/pool.rs`, `backend/crates/worker/src/start.rs`), so searches are reproducible only because Stockfish's defaults are 1 and 16 MB. The bootstrap was seeded on 2026-11-23; nothing stops an unseeded `Math.random` outside `server/stats`.
- The Tier 1 benchmark (ADR 0100) measures the recommendation, but no suite measures the motif tagger, the tablebase judgements or the language layer against a ground truth, and nothing records cost or latency per run.

The owner asked for a factual way to do this, with no hallucinations and with determinism.

## Decision

1. **Four layers, each with its own guarantee and ground truth.** Facts (deterministic; Stockfish, Syzygy, Lichess puzzle themes, injected flaws), profile (deterministic; synthetic players, calibration), recommendation (deterministic and computed with no language model; Tiers 1, 2 and 4), language (never a source of facts; deterministic checks on every output). The language layer may vary in wording, never in which recommendation it names.
2. **One versioned algorithm.** `algorithm/manifest.json` names every component of layers 1 to 3 and the layer-4 prompt sets and models, mapped to the constants in the code. Its id is `algo-<major>.<minor>.<patch>+<first 12 hex of the sha256 of its canonical JSON>`. Major: layer 1 to 3 output can change. Minor: only layer 4 changes. Patch: nothing a replay can see changes. `algorithm/CHANGELOG.md` has an entry per version with the reason, the ADR and the benchmark delta.
3. **The CI guard.** A test in a new `algo` crate rebuilds the manifest from the constants and fails when the committed file differs; `scripts/algorithm/check.mjs` fails a pull request whose components changed without a version bump that fits the policy and a changelog entry. The id is read by writers from `common::algorithm` (`include_str!` of the manifest), so no writer depends on the `algo` crate.
4. **Stamping.** A migration adds `algorithm_versions (id, manifest)` and a nullable `algorithm_id` on `coaching_inputs`, `report_runs`, `weekly_reviews`, `coach_messages`, `focuses`, `drill_items`, `kpi_snapshot`, `weakness_estimates`, `weakness_state_history` and `game_metrics`. `position_evals` keeps its (`fen_key`, `protocol`) key: the eval cache is shared by every version with the same engine protocol. `NULL` means "before versioning".
5. **The determinism contract.** The same games and the same id give byte-identical facts, profile rows and CoachingInput (wall-clock columns and row ids excluded). A replay suite runs the pipeline twice on the committed fixture games and compares canonical dumps with golden snapshots; any diff fails CI unless the version is bumped. The engine runs with `Threads 1` and `Hash 16` sent explicitly, `ucinewgame` before every search and a node limit, checked by an audit test. Randomness in the maths is mulberry32 seeded from the data, and a test bans `Math.random`, argument-less `Date.now()` and `new Date()`, `rand::`, `SystemTime::now` and `OffsetDateTime::now_utc` in the maths modules of both languages.
6. **The evaluation lab.** Every benchmark run writes a RunRecord (the id and manifest, commit, suite, seeds, data checksums, machine, Stockfish build, durations, and token costs for `--live` runs) to a `bench-history` data branch outside the `main` ruleset. `cli bench compare A B` reports per-KPI differences with bootstrap 80% intervals and a verdict. A pull request fails on a clear regression in a guarded KPI: the 80% interval of the difference lies wholly on the worse side. The quality, cost and latency KPIs, and their source columns in `job_metrics`, `ai_usage`, `report_runs` and `games`, are defined in the spec. An LLM judge may be reported; it never gates.
7. **Show your source.** Every recommendation gets a "Why this?" with its facts, its rule and its algorithm id (data in v0.2, UI in v0.5).
8. **The roadmap order.** v0.2 Factual and deterministic foundation, v0.3 The evaluation lab, v0.4 Patterns and progression, v0.5 Board, sound and clarity, v0.6 Practice habit and trainer depth, v0.7 More players, v1.0 MVP. The practice-habit and trainer items move to v0.6, Lichess, languages and 2000+ validation to v0.7, the benchmark-stage items into v0.3, and the phone and screen-reader epic into v0.5. Issues #11, #13 and #14 belong to v0.3.
9. **Amending ADR 0099: the roadmap file is the source of truth for open seeded issues.** `milestones.json` gains `renamed_from` (old titles: the first one found is renamed in place, so its issues and number are kept; any other found is closed, or deleted with `--prune`) and `adopt_issues` (issues filed by hand that the milestone takes; only their milestone changes). `apply-settings.sh --seed-issues` now also moves an open seeded issue to its milestone, adds its missing labels (never removes one) and rewrites its body when the text differs, keeping any box ticked on GitHub. A closed issue is never edited. Sub-issue links pass `replace_parent`, so a child can change epic. The body's footer says so. Before, an existing issue was never edited, so a re-ranked roadmap could only be applied by hand, issue by issue.

## Options considered

1. **A version string per component, no manifest.** Rejected: a dozen versions with no single id cannot be stamped on a row or compared in a benchmark, and constants with no version name (`n0`, `DECISIVE_DW`) would still change silently.
2. **A hash of the whole repository as the id.** Rejected: every README edit would be a new algorithm, and the id would say nothing about what changed.
3. **Make the language model deterministic** (temperature 0, a fixed seed). Rejected: no provider guarantees it, and it is not the property we need. The deterministic check on which recommendation the text names is.
4. **An LLM judge as the language gate.** Rejected: two runs can score the same text differently. It stays advisory, as Tier 3 in ADR 0100.
5. **Store the run history in a database or as release assets.** Rejected: a data branch is public, diffable, needs no service, and the site can read it at build time.
6. **Keep ADR 0099's "GitHub owns an issue after seeding".** Rejected for open seeded issues: the roadmap rewrite moves 30 issues and renames four milestones, which the script could not do. Hand-filed issues stay GitHub's, except for the milestone a milestone adopts.

## Consequences

- Every stored recommendation can be traced to the exact rules that produced it, and a benchmark delta is attached to every rule change.
- Each change to a digested file is at least a patch bump. The replay goldens make a patch bump cheap to prove.
- The first apply of the new roadmap renames v0.2 Practice habit to v0.6, v0.4 Self-host and benchmark to v0.3, and v0.5 More players to v0.7, closes v0.3 Trainer depth (its issues move to v0.6), creates v0.2, v0.4 and v0.5, rewrites the bodies of the open seeded issues, and moves #11, #13 and #14 into v0.3.
- An edit to a seeded issue's body on GitHub is replaced on the next `--seed-issues` run. The place to edit is `roadmap-issues.json`; ticked boxes survive.
- Cross-architecture determinism is not promised: an ARM Stockfish build is a new `facts.stockfish`, so a new major version.
