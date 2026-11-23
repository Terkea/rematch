# 0089. The rating trajectory, and then against now

- Status: accepted
- Date: 2026-11-23
- Findings: F6 (the biggest story in the owner's data is untold), F7 (the coach said "I can't see
  your rating history")

## Context

The owner's rapid rating peaked at about 1,100 in April 2026 and is about 800 now, and monthly
accuracy fell over the same months. No page said so: Progress led with "No change detected yet",
and the coach's input had no rating time series, so it could not answer "what changed since I was
1,100?". Any "then" and "now" comparison has to follow the honesty rules: n and an 80% interval
with every number, and "better" or "worse" only when the evidence separates (ADR 0083 §6).

## Decision

1. **One rule, two implementations, one fixture.** The web's read model
   (`apps/web/src/server/trajectory/model.ts`) and the coach's builder
   (`backend/crates/profile/src/coaching/trajectory.rs`) compute the same thing line for line:
   the same seeds, the same mulberry32 stream and bootstrap as `compareMeans` (2000 resamples),
   the same Beta posteriors and Monte Carlo P(better) as `compareRates`. The web writes
   `backend/fixtures/trajectory/cases.json` (`apps/web/test/trajectory/parity.test.ts`,
   `TRAJECTORY_FIXTURE_WRITE=1`), and `profile/tests/trajectory.rs` replays every case and checks
   every window, verdict and number to 1e-9.
2. **Ratings.** The rated games of one account × time class, each with the player's pre-game
   rating (ADR-FIX-C-1): Lichess stores it; on chess.com it is the stored rating of the previous
   rated game (the first game keeps its own). The current rating is the newest stored one (on
   chess.com the rating after the last game).
3. **Peak.** The first game with the highest rating; the current rating is the peak when it is
   higher still.
4. **Windows.** `W = min(30, floor(n / 2))` games each. "Now" is the last `W` rated games. "Then"
   is the `W` games centred on the peak (clamped so it ends before "now" starts) when the peak
   came before "now" (`basis: peak`), else the first `W` games (`basis: start`). Below 20 rated
   games (two windows of `KPI_MIN_GAMES`) there is no trajectory: the page and the coach say how
   many more rated games are needed.
5. **Kind.** The windows' pre-game ratings are compared with `compareMeans`: `worse` is a fall,
   `better` a rise, anything else flat. A level line still has a highest game, so it is compared
   with the games around it and called "no clear change".
6. **Then against now.** Rating, accuracy, blunders per game and score (`comparePeriodRows`, the
   period comparison's own rows), and the top three claimable weaknesses (`priority > 0`, by
   priority) as events per chance in each window's own games, under the prior behind the stored
   estimate (`n0 = alpha + beta - n`, `mu = (alpha - k) / n0`, else the engine's no-benchmark
   prior). A weakness side needs 10 games with the chance.
7. **Words.** A measure is "clearly worse" or "clearly better" only with that verdict, "no clear
   change" otherwise, and "not enough games to tell" (with no number) below the minimum. The
   headline states facts: "Your rapid rating peaked at 1,097 in April 2026 and is 816 now
   (−281)." A rating moves for many reasons, so the copy and the coach point to a clearly worse
   measure as the likeliest place to look, never as the cause.
8. **UI.** Progress's first section is the trajectory: the headline, the "since then" sentence,
   `RatingTrajectory` (`@ci/ui/charts`: one ink line of the pre-game rating, the peak marked and
   labelled, the current rating at the end, both windows shaded and named on the plot, a
   crosshair readout on pointer and arrow keys, and a table of the months) and the then | now
   table (the period comparison's `CompareBlock`). The Overview gets `TrajectoryCard`, the
   headline and a link to Progress, under its headline; it renders nothing below the minimum.
9. **Coach: coaching-input v3.** `trajectory` (required in v3, absent before): `ratedGames`,
   `sufficient`, `needMore`, `kind`, `peak`, `current`, `change`, `months` (the newest 36, each
   with games, last and highest rating) and `thenVsNow` (both windows, and each measure's `then`
   and `now` as `ColorMetric`s with their unit, `elo`, `pct`, `per_game` or `rate`, its verdict
   and its difference with the 80% interval). The evidence index gives every number its unit
   (`unit_of`): shares and a score difference print as percentages, accuracy and its difference
   with `%`, ratings bare. The chat context lists the trajectory and its paths; build-profile's
   snapshot (ADR 0086) carries it. Prompts: `report/report` v3 and `chat/system` v4 teach it
   (earlier versions stay for stored hashes; the prompt hash records the change).

## Consequences

- The web reads one more query per Progress view (rated games and their window stats), and the
  builder one more per input; neither touches the whole history's moves except for a count per
  rated game in the builder.
- A change to the rule changes both sides and the fixture together; the parity test fails
  otherwise.
- Stored v1 and v2 inputs still parse; the coach prefers the newest version (v3 ranks above v2).
