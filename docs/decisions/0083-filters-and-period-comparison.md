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

## Amendment 1 (T3, 2026-11-22): the filter bar on the pages

1. **One `FilterBar`** (`app/(app)/app/_filters/`) on the overview, games, progress, weaknesses,
   areas and report pages. It is links and GET forms only (native date inputs for the custom
   range and the custom periods), so it works without JavaScript; below `md` it is one "Filters"
   disclosure that names the active filters. A page declares what its data supports: format
   (overview only; the stored series have no format), colour (`on`, or `coming` where the stored
   estimates are Both only), dates, and the comparison (overview and progress).
2. **What a filter changes is said on the page.** On the overview, format, colour and dates drive
   the White | Black row and the comparison (`gameStats`); the headline and charts still cover the
   whole class, and a note says so when such a filter is set. On progress, colour applies to the
   comparison only. Date filters clip the stored weekly series to the weeks that touch the range;
   a dense series keeps its length with gaps (null), never zeros.
3. **Colour on stored rows reads ADR 0082's side.** `CoachScope.side` (default "both") selects
   the side of `weakness_estimates`, `kpi_snapshot` (KPIs and area scores) and the peer rows'
   chances; the pages set it from `color` through `withSide` (`_filters/side.ts`). A colour's
   weekly points and examples are that colour's games (`pointOnSide`). Timeline states have no
   side, so a colour shows its rate, range, n and its own peer comparison but no state badge and
   no better-or-worse call, and its list is ranked by priority, not grouped by state. Without
   its own peer rows a colour reads "No colour benchmark", never Both's. In Both mode each
   weakness row lists White's and Black's rate with their chances. The report stays Both (its
   sections have no side) and its bar says so.
4. **The comparison view** shows A | B | change for accuracy, blunders per game and score (points
   per finished game, compared as a per-game mean by the bootstrap rule of §6), n on each side.
   The change column holds only the verdict's words, and the difference with its 80% range only
   beside a reached verdict; below the minimum it reads "Not enough games to tell: need N more in
   period B".
5. **The games page** reads `color` (with `colour` as an alias) and writes `color`; a `?colour=`
   URL redirects to the same URL with `color`. Its time control, colour and dates come from the
   shared bar, and its own controls keep result, account and sort. Its default class is now the
   first with games by `TIME_CLASS_ORDER` (rapid when there is rapid), as on every other page;
   before, a player without rapid opened on every class. The custom range uses native date inputs,
   replacing the explorer's typed "1 Oct 2026" fields (M5a T13): the GET form must work without
   JavaScript.

## Amendment 2 (2026-11-23): the last 90 days by default

The owner asked for it: a page that opens on every game since the account was linked mixes this
month's chess with last year's, and the numbers a player acts on should describe how they play now.

1. **The default range is the last 90 days** (`DEFAULT_PRESET = "90d"`, `server/filters/presets.ts`):
   today and the 89 days before it, UTC. A URL with neither `from` nor `to` (or with only invalid
   ones) reads as that range, so the default rolls forward with the calendar; a link with days
   still keeps its days. One bound alone keeps the other end open, as before.
2. **All time is explicit: `from=all`.** `serializeFilters` omits the default range and writes
   `from=all` for an unbounded one, so `parse(serialize(f)) = f` still holds. `setRange`
   (`params.ts`) is the one writer every cross-page link uses (the weakness and area links, the
   nav's `withFilters`), so a chosen "All time" travels between pages. `from=all&to=…` is "up to"
   a day.
3. **The presets** are 30 days, 90 days, 6 months, 12 months and All time; "90 days" replaces
   "3 months", which covered nearly the same games. The default 90 days do not count as an active
   filter on the phone disclosure; All time does.
4. **The games list keeps every game** (`defaultPeriod: "all"` in its bar config). It is a list,
   not a statistic, and its "Clear filters" and widening suggestions already mean every game. Links
   from the statistics pages carry their days to it (`gamesPageParams`), and all time arrives as
   no dates.
5. **Too few games say so.** When the default 90 days hold fewer than `KPI_MIN_GAMES` (10) games of
   the class, the bar shows "4 games in the last 90 days. Most numbers here need at least 10." and
   one "All time" link (`FilterBarModel.thin`, counted by `server/filters/window.ts`). A range the
   player chose shows no such note: they asked for it.
6. **The coach and the report state their window.** Neither is narrowed by dates: the report's
   text and sections come from the profile (the weakness estimates cover the last 183 days,
   `ESTIMATE_WINDOW_DAYS`), and the chat reads all the analysed games. The report always says so
   under its bar ("The report is written from your whole profile, on all your games of this time
   control …"), and the coach's lede says the other pages' date filters don't apply. The AI
   pipeline is unchanged.

Consequences: an old bookmark without dates now opens on 90 days, and "All time" is one click
away. The e2e specs whose seeds sit on fixed days open with `from=all`, so they don't drift as the
window rolls.
