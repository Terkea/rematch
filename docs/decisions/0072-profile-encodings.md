# 0072. Profile encodings: weakness rate, KPI small multiples, exemplar arrows, optional next band

Date: 2026-11-22. Status: accepted (M5b Task 4). Builds on ADR 0068 (R3: the weakness headline),
ADR-WEB-12 (KPI stats), ADR-WEB-14 (weakness timeline), ADR-WEB-15 (annotations) and DESIGN.md
("Severity and marks", "KPI stats", honesty rules).

## Context

M5b's coach pages show a player's weaknesses, KPI trends and example positions. The numbers are
posteriors and stored intervals from R2, often thin. Each encoding has to show n, never show an
unknown as 0, show stored intervals exactly, and never let colour carry meaning alone.

## Decision

1. **Weakness rate wording.** `weaknessRateText` reads "Estimated 4.2% of chances (80% range
   3.1–5.6%), from 38 of 902": the stored posterior mean and its stored 80% interval, one
   decimal each, rounded once and never widened or recomputed, with an en dash (U+2013) in the
   range and the raw counts beside them. The row shows the same numbers visually: the mean in
   Literata, the range, and "38 of 902 chances".
2. **Band comparison.** The interval is drawn on a strip from zero (a `rule-strong` band, the mean
   as an `ink` tick, the band median as a short hanging `pencil` tick, AreaScore's median shape).
   The words only say "above" or "below" when the whole interval clears the median ("Band median
   2.1%, below your whole range"); otherwise "inside your range". No median: "No band reference
   yet". An insufficient row (`n_unweighted < min_evidence`) replaces the comparison with "Early
   read" and "N more chances needed", and its state chip is a dashed empty box labelled "Early
   read"; the estimate stays visible, quieter, with its counts.
3. **State chip.** The chip reuses `stateStyle` (colour, pattern and bar weight from the timeline),
   always beside the state's name, so the chip and the swimlane are one code.
4. **KPI small multiples.** `KpiTrendGrid` draws one multiple per metric on **one shared time
   axis** (`range`, by day). Each draws its stored band as a `pencil` 25% area wherever `low` and
   `high` both exist, with unrounded coordinates so its edges sit exactly on `low` and `high`; an
   `ink` line through weeks with `n ≥ minN`; **hollow** dots (an `ink-soft` ring) for weeks with a
   value but `n < minN`, which also break the line; and **gaps** for null values, never zeros. The
   value axis is per metric (its values and band ends, padded by an eighth, clamped to the unit's
   limits) and its ends are labelled. Report dates are dashed `pencil` rules, numbered on the plot
   and listed with their dates under the grid. A key draws every mark in its own shape. Each
   multiple has an accessible summary naming every week ("Blunders per game: 1.20 in week of 2 Nov
   2026, 12 games; no games in week of 9 Nov 2026; …"), including thin weeks ("below the 5-game
   minimum"). A metric with no values says "No games in this range yet" instead of drawing. The
   grid does not animate, so reduced motion needs no other path.
5. **Exemplar arrows.** `ExemplarBoard` shows the position before the move: the played move as a
   `pen` arrow stopping at the target's edge, with its stored glyph as a `pen` badge; the better
   move as a `good` arrow (solid head); the first move of the refutation as a `pencil` arrow (the
   opponent's idea). The caption is in notation and derived from the FEN ("4. Nxe5? Better was
   Nxd4. Win chance 52% to 36%."), each sentence only when its data exists; the UI never
   classifies the move itself. A key under the board names each arrow. The board is static but its
   square grid is a tab stop; the arrows draw on with the pen and appear drawn under reduced motion.
6. **R8: optional next band.** `AreaScore`'s `band.next` is optional. A percentile score has the
   band median at 50 by construction and the next band's position is not measured yet, so without
   it no next-band tick is drawn, the domain ignores it, `areaScoreText` says "next band not
   measured yet" and `AreaScoreKey next={false}` drops its key entry.

## Consequences

- Hosts pass plain structural props; the KPI `unit` is the catalogue's unit code (`rate` 0–1,
  `pct` 0–100, `per_game`, `seconds`, `elo`), formatted by `formatKpiValue`.
- The copy helpers live in `profile/text.ts` without "use client", so server components can call
  them.
- A KPI's `n` counts games by default; a host counting chances passes `nUnit`.

## Amendment (fix round 1, critic and review)

- The row shows "Estimated % of <chances>" and "80% range …" as visible text, and names its counts
  through `counts` (plural nouns). The comparison names the reference group (`band.label`, default
  "Players in your rating band") and reads "lower than / higher than / inside your likely range".
- Rows in a list share one interval scale from 0 (`weaknessScaleMax`, passed as `scaleMax`), with
  labelled ends, so different ranges look different. `WeaknessRowKey` explains the strip.
- An early read is visibly provisional (muted, dash-underlined "Early estimate") and shows no strip and
  no sparkline ("Weekly trend after N more chances"); with no chances there is no figure at all.
- The KPI axis ends are widened to tidy ticks (only the axis; bands stay at their stored ends). The key
  names every mark, including the lone dot and the gap, in the grid's `nUnit`. KPI bands are 80%
  (`INTERVAL_LEVEL`, `Z80` in `profile-core`).
- `AreaScore` takes an optional `interval` (`{ low, high, level }`): drawn and stated as given, `null`
  says "No interval yet", absent says nothing.
- `ExemplarBoard` names the reply's SAN in its key ("Their reply, Qg5"), from the FEN with chess.js.
