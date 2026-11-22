# 0047. Honest overview statistics

Date: 2026-11-22. Status: accepted.

## Context

The overview shows rates and means over the player's analysed games. Samples are often small, and the owner's rule is that no number is shown from zero evidence. Every rate must carry its n, and no number may be NaN or infinite. An interval that cannot be estimated is null, never invented.

## Decision

- **R3, the minimum.** `KPI_MIN_GAMES = 10`. Below it, an estimate is `{ kind: "insufficient", n, needed }` and the page says "Need N more games". Ten is the smallest sample at which an 80% Wilson interval for a rate stays narrower than about 0.4 and a bootstrap of a mean is not dominated by one game. Non-finite inputs are dropped and do not count toward n.
- **Rates use Wilson.** The 80% score interval (`z = 1.2815515655446004`) is well behaved at 0 and 1 and never zero-width for n at least 1. The score of a time class is `(W + D/2) / n` with the Wilson interval on that fraction. Unfinished games (`*`) are excluded from n.
- **Means use a seeded percentile bootstrap.** 2000 resamples, the 10th and 90th percentiles. The seed is FNV-1a of the metric name plus the sorted game ids, feeding mulberry32, so server renders are deterministic.
- **Unestimable means null, never widened.** A sample with no spread (identical values, or all clean games) has no interval: `Estimate.interval` and the rolling `low` and `high` are null, and pages draw the value alone. Intervals are clamped to the metric's domain: counts at least 0, percentages within 0 to 100 (`estimateMean(values, seed, domain)`). Percentile ends are used as computed, with no adjustment toward the mean.
- **Trend.** A rolling mean over the trailing `TREND_WINDOW = 20` games, shown from the point where the window holds at least `TREND_MIN = 10` (amended by ADR-WEB-19: `TREND_MIN = 20`, a full window, so "the last 20 games" is literal). Its band is an analytic 80% Student t interval, not a bootstrap: 491 windows of 2000 resamples cost about 290 ms at 500 games, and the t band is deterministic and free. Single aggregates (overall accuracy, the three judgement means, each class's accuracy) keep the 2000-resample bootstrap, seeded by metric name plus sorted game ids. The overview answers in under 150 ms at 600 games (asserted in a test).
- **R4, weekly trend.** The 12 ISO weeks end in the week of the latest analysed game, not "now". A player returning after a break sees their last stretch, and baselines stay deterministic. A week without games is null, not zero.
- **Only `done` games count.** Pending, analysing, failed and skipped games contribute to the status counts only. A done game with no accuracy is left out of the accuracy statistics, and a done game with no moves rows is left out of the judgement statistics; the two are independent.
- **Aggregation.** Counts are grouped in SQL, in a read-only transaction scoped to the user's links. Bootstraps run in TypeScript over the per-game values. `recent` comes from a second read-only transaction (`listGames`), so it may come from a slightly later snapshot than the counts; that is accepted. Weekly entries carry `games`, the number of analysed games behind them.

## Consequences

Pages never format a number from an `insufficient` estimate and draw no band when the interval is null. The Wilson value for 5 of 10 is 0.3122 to 0.6878 at 80% (hand-checked).
