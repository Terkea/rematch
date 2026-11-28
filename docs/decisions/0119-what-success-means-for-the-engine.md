# 0119. What success means for the engine

Date: 2026-11-28. Status: accepted (lane `benchkpi`, branch `feat/bench-kpis`, the owner-approved plan "KPIs, visualisations and an architecture tournament for the engine", sections 2 and 3). Builds on ADRs 0100, 0104, 0109, 0114 and 0118.

## Context

The owner asked how the engine is benchmarked and whether Rematch is a successful engine. The Tier 1 bench (`backend/crates/bench`, ADR 0100) reports nine scores, but none of them has an interval, two of them cannot move, and one measures the drill catalogue rather than the engine. The arena (`lab/leaderboard.json`, ADR 0109) has intervals but no single tree saying which number matters. A scientist panel (statistical analysis, experimental design, critical thinking) designed the tree below and reported four findings. Each was checked against the committed data before this ADR was written:

1. **Recall at 2 is structural.** Recomputed from EXP-0001's seven arena records (`algo-1.1.0` to `algo-6.0.0`): every version gives 19 of 30, 0.633, with the same split by type: hanging pieces 6 of 6, low-clock blunders 6 of 6, thrown-away wins 5 of 6, opening blunders 2 of 6, missed mates 0 of 6. The panel's "three always found, two never" is close but not exact: two types are always found, one nearly, missed mates never, opening blunders a third of the time. No ranking tune has moved it, because the two weak types fail on their benchmark rows (W4.missed has no peer benchmark at 80 games, W7.traps's cost goes to W1), not on their priority.
2. **`algo-6.0.0` traded detection for fewer false alarms.** Recomputed from EXP-0015's records: false `falling` at 120 games fell from 0.621 [0.577, 0.664] to 0.110 [0.085, 0.142], and detection of a 100-point fall fell from 0.887 [0.869, 0.902] to 0.523 [0.497, 0.549] (200 points: 0.978 to 0.848). `lab/findings.md`'s EXP-0030 row reports the detection figures for EXP-0030's own players but not this drop on EXP-0015's; the trade-off line is plan section 1's.
3. **The two `clean_false_alarms` are different measures, not one wrong one.** EXP-0001's arena counts its two pilot controls (`exp1-clean-0` and `-1`) flagged at any checkpoint up to 80 games: both are, at every version, so 1.0. `bench/baseline.json` counts the smoke suite's one control (`bench-clean`) at its last checkpoint: 0 of 1. Both are computed as documented (`docs/benchmark.md`, "The five scores"); neither can show a 0.05 target, which needs 32 controls (MATH.md K2).
4. **The smoke coverage gate is two-sided at its baseline.** Already proved by EXP-0001's maths audit (`lab/findings.md`, 2026-11-28; EXP-0033 filed). Three tolerances in `bench/baseline.json` (`top1_precision`, `plan_hit_rate`, `top2_agreement`) are marked "not derived" there.

## Decision

1. **The KPI tree.** One tree, from what matters to what drives it:
   - **North Star: a correct focus on real players.** Unmeasurable today; the scorecard shows it as an "unmeasured" row.
   - **NS-2, its proxy (Tier 2):** on anonymised Lichess players (ADR 0114), the share whose rank-1 weakness is one of their top-2 sources of lost points in the next 40 games, against a peer-frequency baseline ranker; the paired Newcombe 80% interval of the difference, about 400 players (EXP-0004, then EXP-0035). Target: the interval clears 0.
   - **Drivers (Tier 1 unless named):**
     - D1a flaw types detectable: types whose recall at 2 has its 80% Wilson lower end at or above 0.8 (today 3 of 5 by point estimate on EXP-0001; none can clear by interval at 6 a type, K2). Target: all five.
     - D1b macro recall at 2: the mean of the per-type recalls, with a stratified bootstrap. Target: at least 0.8.
     - D2 worst-cell calibration: every weakness by sample-size cell of EXP-0002 in [0.75, 0.85].
     - D3 false weakness on clean players: at most 0.05, which needs 32 clean controls (EXP-0013).
     - D4 the trajectory pair: false alarm at most 0.10 and detection not lower than the champion's; both halves are always reported (finding 2).
     - D5 the Kaplan–Meier median games to a top-2 detection: at most 45.
     - D6 stability, top-2 Jaccard: at least 0.85.
     - D7 cost: engine CPU-seconds per game (at most 40) and the 95th percentile time to analysed (`cli kpi`, #67, bench runs only, never app users).
   - **Guards:** determinism (exact); trusted-motif precision at least 0.80 (EXP-0007); the peer-rate coverage band (EXP-0006).
2. **What the bench computes** (`backend/crates/bench/src/kpi.rs`, new; `backend/crates/bench/src/results.rs`; `backend/crates/bench/src/lib.rs`). Every KPI as `{value, interval, level, n}` at 80%, the leaderboard's shape, in the scorecard's `kpis` map:
   - `recall_at_2` stays pooled, computed exactly as before (EXP-0001 reads it), now with its Wilson interval;
   - `recall_at_2.<flaw>` per type (Wilson), `flaw_types_detectable` (D1a, by lower end) and `flaw_types_detectable_point` (by point estimate);
   - `recall_at_2_macro` (D1b), stratified bootstrap of 2,000 resamples, mulberry32 seed 119;
   - `km_median_games` and `km_median_games.<flaw>` (D5), Kaplan–Meier on the checkpoint grid with Greenwood's variance, undetected players censored at the last checkpoint, and the curves in `km_curves`. It replaces `mean_games_to_detection`, which stays in the JSON for one version, marked deprecated;
   - `top2_jaccard` (D6; `top2_agreement`'s value) with a bootstrap interval, `coverage` with a player-cluster bootstrap by flaw, `clean_false_weakness` (D3, Wilson), `deterministic` (level `"exact"`).
   - The Markdown scorecard says that every Tier 1 number is synthetic and lists the KPIs with their intervals and n.
3. **Kept, dropped, changed.**
   - Kept: pooled recall at 2, coverage and the clean count, byte-identical, with the smoke gate against `bench/baseline.json` unchanged.
   - Changed: recall at 2 is read as macro plus per type; the mean games to detection becomes the Kaplan–Meier median.
   - Dropped as a KPI: `plan_hit_rate`. It is at least recall at 1, and equal to it unless another key at rank 1 shares the flaw's drills, so it measures the catalogue. It is still reported, labelled "not a KPI", and the unit test `every_rankable_key_has_a_drill` holds what it was checking: every key in `RANKABLE` has a drill of its own, and every flaw's drills are in its target's list.
   - To retire later: the smoke coverage gate, after EXP-0033; coverage is then gated by EXP-0002 in the arena.
4. **The maths is in `algorithm/MATH.md`, "KPIs" (K0 to K5)**, per ADR 0118: each formula, the interval and its proof sketch (Wilson; the stratified bootstrap; Kaplan–Meier and Greenwood; the paired comparison), the power at the counts used and the target's reasoning, checked by `algorithm/math_check.py` (sympy identities, exact binomial coverage, simulations with a known truth).
5. **History and comparison.**
   - `cli bench run --record FILE` appends a `run-record-v1` (suite, date, `algorithm_id`, commit from `GITHUB_SHA`, a digest of the players and seeds, tier `synthetic`, engines, nodes, the KPIs) to a `bench-history-v1` JSON, the shape `site/scripts/kpi-charts.mjs` validates and reads.
   - `cli bench compare A.json B.json [--json]` differences two `bench-results.json` files metric by metric, B minus A, with an 80% bootstrap interval: paired by player when both runs have the same players on the same seeds, unpaired otherwise. The verdict ("higher", "lower", "no clear difference") is read from the interval only.
   - `.github/workflows/bench.yml`'s full suite (weekly and on dispatch) writes `bench-out/bench-history.json` into its artifact. Its permissions stay `contents: read`; nothing new is pinned.
   - Still to do: a later step pushes each record to a `bench-history` branch. That needs a write permission and its own review, so it is not part of this decision.
6. **The arena.** `backend/crates/bench/src` is EXP-0001's `SOURCE_PATHS`. The changed files are accepted for EXP-0001 by entries in `lab/source-reviews.json` citing this ADR (ADR 0109 Amendment 1): `backend/crates/bench/src/kpi.rs` (new; called only by the scorer, after the metrics EXP-0001 reads), `backend/crates/bench/src/lib.rs` (`pub mod kpi`) and `backend/crates/bench/src/results.rs` (the KPI map, the units, the Markdown table and two tests). The players, their checkpoints and the pooled recall at 2, coverage and clean count are unchanged: a smoke run before and after the change gives byte-identical players and identical values of those fields (shown in the review entries' reason).

## Options considered

1. **Keep the nine scores and add intervals.** Rejected: pooled recall would still hide the never-found type, the mean games to detection would still drop the undetected players, and plan hit would still look like an engine score.
2. **Report macro recall only.** Rejected: EXP-0001 and the CI gate read pooled recall, and changing a measured metric is a new experiment.
3. **A log or log-log Greenwood band for the median.** Considered. On a four-point grid the plain band is already conservative (K4 simulation, 0.989), and it is the one the proof sketch covers; a transformed band is a later refinement if the grid gets finer.
4. **Compare runs on their run records.** Rejected: a record holds summaries, so it cannot pair players. `compare` reads the full results, where each player is there.

## Consequences

- Every Tier 1 number has an interval and an n, and the site's scorecard can read the bench history with the same renderer as the leaderboard.
- D1a by interval reads 0 on any suite with fewer than 7 players a type, whatever the engine does (K2). The full suite (22 a type) is the one that can show it.
- `bench-results.json` gains `scorecard.kpis` and `scorecard.km_curves`; the players and every older field are unchanged, so EXP-0001's analysis reads it as before.
- The site's D6 row describes top-2 Jaccard "between adjacent checkpoints"; what the bench measures is with and without 10% of the games. The site's wording is a follow-up.
