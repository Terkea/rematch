# ADR-WEB-12. KPI stats: a fixed percent window, outward-rounded interval ends, a host-set halo and an honest trend scale

Date: 2026-11-21
Status: Accepted (amended by controller rulings in the Task 14 review)

## Context

Task 14 built `Sparkline`, `Stat` and `KpiStat`. Every KPI carries a credible interval from R2's statistics (Wilson for rates, bootstrap for accuracy), and the spec asks us to show uncertainty honestly. On a 0–100% scale, an accuracy band of 4.8 points was a sliver hidden by the value tick, while a per-row auto-zoom would make every band look equally wide. The first version also printed interval ends at rounded values, defined the halo token on the root, and scaled each sparkline to its own range, which made noise look like progress.

## Decision

- **Every percent KPI gets a 40-point window, never 0–100% (ruling 1).** When the band, value and target fit, the window is centred on them, its start rounded to 10 points (widened back if rounding would cut them). Otherwise it is centred on the value, and it is always clamped inside 0–100%. An interval wider than the window runs off the track's end, square-cut, with an open chevron ("interval continues"), so a wider interval always looks wider. The text stays exact. A target outside the window is named only in text. Other formats run from zero (or below, for negatives, including a negative target) to a nice value about 15% past the band, value and target.
- **Interval ends round outward (ruling 2).** The low end is floored and the high end ceiled at the shown precision, so the stated range is never narrower than the computed one and never crosses the rounded value. Decimal ends show one place fewer than the value (`64.20`, "between 61.0 and 67.5"). Percent and integer ends keep the value's precision. A target that would read the same as an end gets one more place.
- **`--stat-surface` follows the eval graph's pattern (ruling 3).** Components read `var(--stat-surface, var(--color-sheet))`. Neither they nor `tokens.css` define it, so a host's value always wins.
- **The sparkline spans at least twice the current 80% interval (ruling 4),** centred on the series range. `Sparkline` takes an optional `minSpan`, and `KpiStat` passes it (a 90% interval is scaled by 1.2816 / 1.6449). A move smaller than the uncertainty therefore looks small.
- **Smaller rulings.** The provisional band is not faded. The target label is "Next target". The sparkline reveals through one clip, so the pieces after a gap draw in order. The not-enough-games fallback names the KPI. The heading level is a prop.
- **One clock per value change.** The NumberFlow roll and the value tick share `--dur-reveal` and `--ease-pen`, driven by one progress value. Reduced motion jumps both to the end.

## Consequences

- Two percent KPIs with different centres sit in different windows. The labelled ends show this, and widths still compare.
- The trend of a noisy KPI looks flat. That is intended: it isn't a measured change.
- Changing the window size or the 2× factor is a design change recorded here, not a tweak.
