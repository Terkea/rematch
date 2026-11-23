# 0091. A weekly focus with a measurable goal

Date: 2026-11-23. Status: accepted (focus lane, owner-approved feature). Builds on ADR 0068 (the
profile read model), ADR 0082 (the colour split) and ADR 0084 (the costing state the peer
comparison backs). Numbers 0088 to 0090 are taken by parallel lanes (0090 is the trainer).

## Context

The end-user review of 2026-11-23 found the app a good mirror and a weak coach: nothing says
"your one thing this week", and nothing shows whether it worked. The research
(`docs/research/2026-11-23-what-helps-amateurs-improve.md`) says specific, hard goals beat "do
your best", but only with progress feedback; feedback should be about the task, given as a rate
per chance, and process should come before outcome.

## Decision

1. **A focus** is one weakness series per user and time class (one running row per
   `(user_id, time_class)`, a partial unique index), on one of the user's accounts and a side
   (`both` by default; a colour when set from a page filtered to one). It is stored in
   `focuses` (migration `20261123043000_focuses.sql`): the series, the target rate, N (`games_n`,
   default 20, 5 to 100), `started_at`, the baseline (the estimate's rate and its unweighted
   k/n over the last 6 months), the peer median it was weighed against, and on closing
   `ended_at`, `outcome` (`met`, `not_met`, `inconclusive`, or `cleared` when the user cleared or
   replaced it) and the counts it ended with.
2. **The target** is the current rate less a meaningful step: the peer median or a quarter fewer
   (`RELATIVE_STEP = 0.25`), whichever is easier, that is the higher rate. A peer median counts
   only when it is at least 10% below the current rate; a rate of 0 or an unknown rate has no
   target. It is rounded to a tenth of a percentage point and always chosen on the server from
   the stored estimate; a client's number is ignored.
3. **The default suggestion** is the top weakness under "Costing you games": a compared, current
   costing state its peer comparison backs (confidence at least 0.7, ADR 0084), with sufficient
   evidence and priority above 0 (the overview's lead). The player can accept it, pick N (10, 20,
   30 or 40), choose another weakness ("Make this my focus" on its detail page) or clear it.
4. **Progress** is read live, never stored while the focus runs: the first N analysed games
   (`game_metrics`) of the account, time class and side played after `started_at`, and the
   series' events and chances in them (`weakness_series_point`). The card shows k of n, the rate
   with its 80% Wilson interval, and games played out of N. Below 10 chances
   (`MIN_VERDICT_CHANCES`) the verdict is "Too early to tell: X more chances"; from 10, "On track"
   (rate at or below the target) or "Not yet". It never says "improved": it compares with the
   goal, and the interval is printed beside it.
5. **Closing.** When the N-th game exists the next read closes the row, dated by that game, with
   `met` or `not_met` (as the verdict), or `inconclusive` under 10 chances, and the card offers
   the next focus. A closed focus stays on the card until the player sets the next one.
6. **Isolation.** Every read and write finds rows by `user_id` and checks the account is one of
   the user's links; another user's focus is never read or changed, and a POST for a foreign
   account is a 404.
7. **UI.** `FocusCard` (`apps/web/src/components/focus/focus-card.tsx`) sits first on the
   overview (a `focus` slot; a later lane recomposes the page) and on Progress. It shows the plain
   name, the goal sentence, a games bar (`role="progressbar"`), the rate against the target with
   n, the verdict as a pill whose words carry the meaning, and "Practise this", a link to
   `/app/train?source=<seriesKey>` (the trainer, ADR 0090). Weakness pages get one
   `MakeFocusButton` under "What to do". `POST`, `PATCH` (N) and `DELETE` `/api/focus` back them.
8. **Weekly review.** Each review gets a computed "Your focus this week" line, never written by
   the model: the practice sessions first (distinct days with a trainer attempt on the account's
   positions, only where the trainer's `drill_attempts` table exists), then the focus's k of n in
   that week's games with the 80% range and the target.
9. **Coach awareness is left out.** It would need coaching-input changes, and the trajectory lane
   is moving coaching-input to v3; a later change can add the focus there.

## Consequences

- The goal is honest by construction: the target and the n behind it come from the same stored
  estimate the weakness page shows, and a thin sample says so instead of a verdict.
- With rare chances (a missed mate comes up about every second game) a 20-game focus can close
  inconclusive. The player can pick a longer N; the card says how many chances are missing.
- The baseline is the estimate's (decay-weighted) rate while progress is the raw share in the
  focus's games; both are shares of chances, and both are printed with their counts.
