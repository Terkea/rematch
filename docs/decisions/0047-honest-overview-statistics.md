# 0047. Honest overview statistics

Date: 2026-11-22. Status: accepted.

## Context

The overview shows rates and means over the player's analysed games. Samples are often small, and the owner's rule is that no number is shown from zero evidence. Every rate must carry its n, and no interval may be zero-width, NaN or infinite.

## Decision

- **R3, the minimum.** `KPI_MIN_GAMES = 10`. Below it, an estimate is `{ kind: "insufficient", n, needed }` and the page says "Need N more games". Ten is the smallest sample at which an 80% Wilson interval for a rate stays narrower than about 0.4 and a bootstrap of a mean is not dominated by one game. Non-finite inputs are dropped and do not count toward n.
- **Rates use Wilson.** The 80% score interval (`z = 1.2815515655446004`) is well behaved at 0 and 1 and never zero-width for n at least 1. The score of a time class is `(W + D/2) / n` with the Wilson interval on that fraction. Unfinished games (`*`) are excluded from n.
- **Means use a seeded percentile bootstrap.** 2000 resamples, the 10th and 90th percentiles. The seed is FNV-1a of the metric name plus the sorted game ids, feeding mulberry32, so server renders are deterministic.
- **No zero-width bands.** A bootstrap of identical values has no spread, so a drawn band is widened to at least 0.05 on each side. `bootstrapMean` itself stays raw (one value gives a point), and only the estimate and rolling layers expose a band.
- **Trend.** A rolling mean over the trailing `TREND_WINDOW = 20` games, shown from the point where the window holds at least `TREND_MIN = 10`.
- **R4, weekly trend.** The 12 ISO weeks end in the week of the latest analysed game, not "now". A player returning after a break sees their last stretch, and baselines stay deterministic. A week without games is null, not zero.
- **Only `done` games count.** Pending, analysing, failed and skipped games contribute to the status counts only. A done game whose player has no accuracy or no analysed move contributes to neither the accuracy nor the judgement statistics.
- **Aggregation.** Counts are grouped in SQL, in a read-only transaction scoped to the user's links. Bootstraps run in TypeScript over the per-game values.

## Consequences

Pages never format a number from an `insufficient` estimate. The Wilson value for 5 of 10 is 0.3122 to 0.6878 at 80% (hand-checked). The widening floor means a perfectly flat series still shows a visible band.
