# ADR-WEB-12. KPI stats: a shared percent window, coarser interval ends and a stat surface halo

Date: 2026-11-21
Status: Accepted

## Context

Task 14 built `Sparkline`, `Stat` and `KpiStat`. Every KPI carries a credible interval from R2's statistics (Wilson for rates, bootstrap for accuracy), and the spec asks us to show uncertainty honestly. Three choices weren't covered by DESIGN.md. First, on a 0–100% scale an accuracy band of 4.8 points was a sliver hidden by the value tick, while a per-row auto-zoom would make every band look equally wide. Second, the interval ends could be shown at the value's precision. Third, the tick halos needed a surface colour.

## Decision

- **A shared 40-point window for percent KPIs.** It is placed around the band, value and target, rounded to 10 points and clamped inside 0–100%. It falls back to 0–100% when they span more than 30 points. Band widths compare across rows, and both ends are labelled. Other formats run from zero to a nice value about 15% past the band and target.
- **Interval ends are one decimal place coarser than the value for decimals** (`64.20`, but "between 61.0 and 67.5"). Percent and integer ends keep the value's precision. A credible interval's ends are less certain than its centre, so printing them to the hundredth would claim precision we don't have.
- **`--stat-surface`** (in `tokens.css`, default `var(--color-sheet)`): the 2px halo behind the value and target ticks and the sparkline's last dot, so each mark's neighbour is the surface (DESIGN.md's halo rule). A host on another surface sets it, as with `--eval-graph-surface`.
- **One clock per value change:** the NumberFlow roll and the value tick share `--dur-reveal` and `--ease-pen`, driven by one progress value, so the tick travels with the number. Reduced motion jumps both to the end.

## Consequences

- Two percent KPIs with different centres sit in different windows. The labelled ends make this visible, and widths still compare.
- A percent KPI whose band, value and target span more than 30 points shows the full 0–100% scale.
- DESIGN.md "KPI stats" holds the rules; changing the window size is a design change, not a tweak.
