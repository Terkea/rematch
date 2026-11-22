# 0083. One filter URL contract, and honest period comparison

Date: 2026-11-22. Status: accepted (next round, T2a). Builds on ADR-WEB-19 (one statistics
convention), ADR-R2-5 (posteriors) and ADR-FIX-C-1 (phase accuracy). Library code only; the
filter bar and pages come in T2b and T3.

## Decision

1. **One URL contract** (`apps/web/src/server/filters/params.ts`, zod): `tc` (rapid first and
   the default, `lib/time-class.ts`), `fmt`, `from`, `to`, `color=both|white|black`,
   `cmp=month|30d|50g|custom`, and `a_from`/`a_to`/`b_from`/`b_to` only with `cmp=custom` (both
   sides need a bound, or the comparison is dropped). Invalid or repeated values fall back to
   their defaults, a reversed range is swapped, and `serializeFilters` writes only valid
   non-default values in that key order, so `parse(serialize(f)) = f`. The games page's `colour`
   is read as an alias of `color`, and `gamesPageParams` maps a scope back to the games page.
2. **Formats** are keyed by raw seconds (`600+0`, shown "10+0") from `games.base_seconds` and
   `increment_seconds` (a missing increment is 0). A class lists its 5 most played, then
   "Other" with the rest and the games of unknown base. Daily has none. A `+` that decodes as a
   space is read back as `+`.
3. **Presets** (30d, 3m, 6m, 12m, all, custom) are shortcuts that write `from`/`to` days; the
   URL keeps the days, so a shared link always shows the same games. `presetOf` names a range
   again.
4. **Comparison periods**: A is the earlier side, B the later. `month` is this calendar month so
   far against the whole previous month; `30d` the last 30 days against the 30 before; `50g` the
   newest 50 games against the 50 before; `custom` the given ranges. They replace `from`/`to`;
   the class, format and colour filters still apply.
5. **The Beta posterior is ported to TS** (`server/stats/posterior.ts`) line by line from
   `profile-core`, with its special functions, quantiles, mulberry32 sampling and P_imp. A Rust
   test writes `profile-core/tests/fixtures/posterior_cases.json` (a fixed grid), and the TS
   test checks every case to 1e-9. A closed window uses the same prior `{mu, n0}` on both
   sides with unweighted counts (no decay).
6. **A change is called only on evidence** (`server/stats/compare.ts`):
   - both sides need `KPI_MIN_GAMES` (10) games, or the verdict is `not_enough_games` with the
     short side and how many more games each needs;
   - a rate is `better` (or `worse`) when P(B better) (or P(B worse)) from the two posteriors is
     at least 0.8 and the posterior means differ by at least 15% relative to A (the timeline's
     `CONFIRM_P_CHANGE` and `CONFIRM_REL_CHANGE`); otherwise `no_clear_change`;
   - a mean is `better` (or `worse`) when the seeded bootstrap 80% interval of `mean_b - mean_a`
     excludes 0; otherwise `no_clear_change`.
7. **Game-level stats** (`server/stats/game-stats.ts`) give n, analysed games, accuracy, own
   blunders, mistakes and inaccuracies per game, score with W-D-L and a Wilson interval, and
   phase accuracy, overall and per colour. Unknown values are dropped (never 0), non-finite
   ones are not counted, and phase accuracy reads only `game_metrics` rows of version 3 (the
   fixed formula).

## Consequences

- A Rust change to the posterior numerics fails the fixture test until the fixture is
  regenerated (`UPDATE_SNAPSHOTS=1`) and the TS port matches it again.
- Presets do not roll forward: a saved "last 30 days" link keeps its days.
