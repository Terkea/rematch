# ADR-R2-7. KPI catalogue, snapshots and area scores

Date: 2026-11-21
Status: Accepted (R2 Task 15; port of TS M2 Task 19 / ADR 0040)

## Context

The profile shows players a set of numbers (KPIs) and seven area scores, each with honest uncertainty. The web's `KpiStat` (ADR-WEB-12) draws an 80% interval around a value and needs to know when there are too few games. The coaching-input contract (R-2) takes only area rows that have a contract unit. Snapshots go into the append-only `kpi_snapshot` table (`double precision` columns), and change detection compares stored rows. The code is in `backend/crates/profile-core/src/kpi/`.

## Decision

### Catalogue (`kpi_catalogue()`)

- **Base rows.** There are 36 base rows, in `BaseMetric::ALL` order. Their metric ids, kinds, areas, directions and minimum N follow the TS M2 Task 19 table. After them come 16 `motif_miss_rate:<tag>` rows (rate, tactics, lower is better, min N 20 opportunities) and 17 `weakness:<key>` rows, one per binomial key (`RANKABLE`). The weakness rows are rates in `weakness_area(key)`, lower is better, with `min_n = N0[key]`. W9d and W12 are continuous and have no row.
- **Contract units (R-2).**

  | Unit | Rows |
  |---|---|
  | `elo` | `rating` |
  | `pct` | `accuracy*`, `quiet_loss_per_move`, `eval_after_opening`, `first_game_accuracy`, `later_games_accuracy` |
  | `per_game` | `*_per_game`, `tb_errors_per_endgame` |
  | `rate` | every rate row, `score_after_loss`, `score_after_win` |
  | `seconds` | `time_per_move.*`, `time_in_opening` |
  | none (internal only) | `perf_vs_expected`, `eval_trend_15_35`, `critical_time_ratio`, `book_exit_ply`, `session_accuracy_decline` |

- **Extraction.** Each row extracts one game's contribution as `Value(x)` or `Counts(events, opportunities)`, or `None` when the game is no evidence for the row. `None` keeps the game out of the denominator; it never counts as zero. The extraction rules:
  - Per-game counts need at least one analysed move.
  - Time rows need a clocked game: not daily, and `time_trouble` is `Some`. Games without clocks are absent, not zero.
  - `tb_errors_per_endgame` needs `tb_positions > 0`. The episode rates need an episode with a known outcome.
- **Session rows.** `SessionInfo` carries only `after_loss`, so `score_after_win` means "after a game in the session that was not lost" (a win or a draw); its label says so. `later_games_accuracy` covers session index ≥ 2. `session_accuracy_decline` = mean accuracy at index ≥ 4 − mean at index 1.

### Windows and aggregation (`compute_kpis`)

- **Windows.**
  - `LastN(50)` takes the 50 most recent games by `played_at`, with key `"last50"`.
  - `Week(key)` takes the games whose UTC ISO 8601 week (`iso_week_key`, for example `"2026-W47"`, or `"2026-W53"` for 2027-01-01) equals the key.
- **Order.** Games with a stale `PlayerGameMetrics.version` are dropped first. Their defaults would read as clean zero evidence. Then games are ordered by `played_at` (newest first, ties broken by `game_id`), duplicate `game_id`s are dropped, and the window is applied. The output is therefore a function of the set of games, independent of input order. Recomputing gives bit-identical rows, so change detection sees no spurious change.
- **Aggregation by kind.**
  - `Mean` gives the mean with an 80% percentile bootstrap (seed 1, 1000 resamples, `None` below 2 values).
  - `Rate` gives pooled `Σk/Σn` with an 80% Wilson interval. `n` counts opportunities, and zero opportunities give no value.
  - `Last` gives the newest value, with no interval and no adjustment.
  - `session_accuracy_decline` uses a two-sample bootstrap, resampling each group on its own. It needs at least 2 games in each group for an interval and at least 1 in each for a value.
- **Honesty rules.**
  - With no evidence, `value`, `value_adjusted` and the interval are all `None`.
  - Non-finite values and impossible counts (`k > n`, negative) are skipped and counted in `SkipCounts`, together with stale versions and duplicates. `compute_kpis_report` returns these counts.
  - NaN and ±inf never reach output, and -0.0 is normalised to 0.0.
  - The interval is widened, never narrowed, to bracket the value. ADR-WEB-12 rounds ends outward and assumes `low ≤ value ≤ high`. A percentile bootstrap does not guarantee that for small skewed samples.
  - `n` is always reported, so the web and the builder can say "have n, need min_n".

### Opponent adjustment

`value_adjusted = value − opp_slope_per_100 × mean((opp − own)/100)`. The mean runs over the contributing games that know both ratings, and is 0 when none does. The adjustment needs a reference stat with a finite slope, so it is `None` without a reference and for `Last` rows. It is not clamped: it feeds band percentiles only (R-2).

**Deviation from the TS text.** TS shifts the stored interval. Here `ci_low`/`ci_high` stay the interval of the raw `value`, because the contract's `low`/`high` and the web's display bracket the raw value. The shifted interval comes from `KpiValue::adjusted_interval()`: the raw ends plus `value_adjusted − value`.

### Snapshots and comparison

- **Append rule.** `kpi_snapshot` is append-only. A new row is written only when `value` or `n` changed against the latest row for the same metric and window (`should_append`). An interval or adjustment that moved on its own, for example after a new reference build, is not a new snapshot. All values are computed in f64 and stored in `double precision`, so a value read back compares exactly.
- **Comparison.** `compare_kpi(a, b, def)` compares the later `b` with the earlier `a` on raw values. It reports a change when the 80% intervals separate, or, for rates, when a pooled two-proportion |z| ≥ 1.96. The direction comes from `higher_is_better`. A row with no direction, a missing value or a different metric gives `NoClearChange`.

### Area scores (`compute_area_scores`)

- **Components.** `area_components` is TS M2's `AREA_COMPONENTS`.
- **Counting.** A component counts when its row has a value, `n ≥ min_n`, the reference has a stat for it, and every input is finite. Its score is `percentile_from_quartiles(value_adjusted, falling back to value)`, which lies in [1, 99] and is reversed for lower-is-better metrics (ADR-R2-5).
- **Sufficient** means at least half the components count and at least one does. Then `score` is the mean percentile, `n` is the smallest counted `n`, and `need_more` is `None`.
- **Insufficient** gives `score None`. `n` and `need_more = min_n − n` come from the first component short of its minimum, where a missing row counts as `n = 0`. When no component is short, the gap is a missing benchmark, so `n` is the first component's and `need_more` is `None`.
- **Mindset** has no benchmark in R2: `score None, sufficient false, need_more None`, with `n` of its first component. All seven areas are always returned in contract order, so an empty account still yields a valid `areaScores`.

## Consequences

- An area is never scored from zero evidence or without a benchmark. A thin account gets seven unscored areas with have/need counts.
- `score_after_win` mixes in draws until `SessionInfo` carries the previous result.
- Changing a minimum N, a component list or the interval level changes what players see, and is recorded here.
