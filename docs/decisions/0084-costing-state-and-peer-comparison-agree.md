# 0084. A costing state and its peer comparison never contradict each other

Date: 2026-11-22. Status: accepted. Bug hunt N10. Builds on ADR-R2-6 (timeline states), ADR 0060
(focus threshold and the "probably" ladder), ADR 0080 and ADR 0081 (the pooled peer benchmark).

## Context

On the owner's rapid profile, "Allowed mate" (W4.allowed) was `active`, listed under "Costing you
games", while its row read "Players you've faced, rated about 667–967: 0.3% (pooled over 1,484 of
their chances), inside your likely range". The player's rate was 0.52% (80% range 0.29–0.78%).

What the stored rows show (read-only on the owner's stack):

- The benchmark is the current pooled one (`account_benchmark_stat`: 0.3498%, 58 opponents, 1,484
  chances); the estimate's `band_median` is the same value. Nothing was compared with a stale
  benchmark from before ADR 0081.
- The state comes from the timeline's confidence rule: the transition to `active` (built after the
  pooled change) has evidence k = 13, n = 3,066 and `P(rate > 0.35%)` = 0.718, over the 0.7 of
  `ACTIVE_CONFIDENCE`. The estimate (183 days, 60-day half-life) has confidence 0.81. Both agree
  that the player is probably above the peers.
- The page judged differently: it calls a difference only when the peers' rate is outside the 80%
  interval, which is roughly a one-sided 0.9. Between 0.7 and 0.9 the state says "costing you
  games" and the row says nothing more than "inside your likely range".
- A second row shows the other way the two can part: "Opening traps" (W7.traps) is `active` at an
  estimate confidence of 0.547. It entered `active` on 2026-04-20 at 0.72 and stays there by the
  timeline's hysteresis (a tracked state holds while the segment's rate is above the median), on a
  segment that runs back to May 2025, while the estimate weighs the last 6 months.

So the state machine follows its rule, which is the spec's: `active` is "the Confidence rule in
improvement-engine §6.3", and §6.3 sets the threshold at 0.7 (also the coach's focus threshold,
ADR 0060, whose ladder calls 0.7 to 0.9 "probably"). The contradiction is on the page, which read
a stricter test than the state and ignored the stored confidence.

## Decision

1. **The detector is unchanged** (`tl-v1`). `active` at `P(rate > benchmark) >= 0.7` on the
   current segment, and the hysteresis that keeps a tracked state while the segment rate stays
   above the median, are the spec's rule and ADR-R2-6's (whose note 17 explains why the
   hysteresis exists). Restricting the timeline's peer tests to the estimate's window was
   considered and left out: it would bump the detector version and replay every history, and the
   hysteresis would still let the two numbers part, so the page has to handle it anyway.
2. **The page reads the stored confidence with the state's threshold** (`PROBABLY_ABOVE = 0.7` in
   `@ci/ui/profile`). When the benchmark is inside the 80% range and the confidence is at least
   0.7, the row says "inside your likely range, though yours is probably higher (81% likely)", the
   detail header the same, and the detail text "Probably more often than players you've faced …
   (0.3%): about 81% likely, though their rate is inside your likely range". Outside the range the
   wording is unchanged ("lower" or "higher than your whole likely range").
3. **"Costing you games" holds only rows the comparison backs.** A compared, sufficient row whose
   state is `active`, `emerging` or `relapsed` but whose stored confidence is below 0.7 goes to a
   new group, "Worth watching", whose lede says the game-by-game record marks it but the last 6
   months are not clearly worse than the comparison group. Its detail page adds "Active comes from
   your game-by-game record (every game since the last change it found), not from this
   comparison." The state chip still shows the stored state: the page explains it instead of
   hiding it.
4. **The overview's lead** ("What to work on") skips the same rows, as the coach's focus selection
   already does (confidence >= 0.7, ADR 0060).

## Consequences

- On the owner's rapid profile, "Allowed mate" stays under "Costing you games" and now says why
  (81% likely); "Opening traps" moves to "Worth watching".
- Every place that says a weakness costs games now uses the same threshold: the timeline, the
  coach's focus, the overview lead and the weaknesses page.
- No Rust change, no rebuild needed for the copy; a rebuild only refreshes the numbers.
- The colour split's per-side rows still carry the account's (`both`) state; they are not shown on
  these pages yet. When they are, they need the same rule against their own confidence.
