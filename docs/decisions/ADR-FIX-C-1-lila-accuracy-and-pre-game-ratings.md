# ADR-FIX-C-1. Lila game accuracy, phase accuracy by the game formula, pre-game ratings

- Status: accepted
- Date: 2026-11-22
- Lane: fix/c (placeholder number; the controller renumbers it)

## Context

An independent audit of the analysis (516 games, Stockfish 19 re-run and recomputation from the
raw data) found four correctness bugs in the numbers we derive from correct evals:

- **A2.** Game accuracy zeroed its harmonic half when one move had accuracy 0. In the audited
  data this understated 45 games by 5 to 18.5 points, and the overall mean was 66.15 where lila
  gives 66.96.
- **A4.** The volatility window was sized as positions / 10. Lila sizes it as moves / 10. This
  moved 15 sides by more than 1 point, and one by 10.7.
- **A3.** Phase accuracy (opening, middlegame, endgame) was a plain mean of per-move accuracies.
  So it sat on a different scale from game accuracy (86 / 82 / 91 beside an overall 63), and
  decided positions inflated the endgame figure.
- **A5.** The rating-predicted score used the stored ratings. Chess.com stores post-game ratings,
  which pulled the score-vs-expected delta toward zero by 0.2 to 1.6 points.

## Decision

1. **Game accuracy is lila's.** We read `modules/analyse/src/main/AccuracyPercent.scala` in
   lichess-org/lila and scalalib's `Maths.harmonicMean`. It is quoted in `game_accuracy.rs` and
   `game-accuracy.ts`:
   - `windowSize = ((cps.size - 1) / 10).squeeze(2, 8)`. `cps` includes the initial position, so
     the window comes from the number of moves.
   - `harmonicMean = a.size / a.foldLeft(0d)((acc, v) => acc + 1 / Math.max(1, v))`. Each move is
     floored at 1, so one zero never zeroes the mean.

   Rust and TS change together and stay in parity. Both gain `game_accuracy_from` /
   `gameAccuracyFrom`, for a series whose first move is Black's. It returns `None` / `null` when
   the colour made no move.
2. **Phase accuracy uses the game formula.** We follow lila `AccuracyPercent.phaseAccuracies`: a
   phase's value is `gameAccuracy` over that phase's slice of the eval series. The slice's first
   mover is the start colour. The series is White's W before the phase's first move, then after
   each of its moves. The value is `None` in three cases: the player made no move in the phase
   (the formula is undefined there), a move of the phase is unusable, or the phase's plies are not
   consecutive. With at least one own move the value is lila's, as Lichess shows it. The labels
   ("Opening accuracy" and the others) now name a number on the same scale as "Accuracy", so the
   copy stays.
3. **The expected score uses pre-game ratings** (`profile_core::weakness::sessions::pre_game_ratings`):
   - **Lichess:** the stored rating is already pre-game. The API's `players.*.rating` is the
     rating at the start, and `ratingDiff` is the change, so we do not subtract `ratingDiff`.
   - **Chess.com:** the player's pre-game rating is their rating after their previous rated game in
     the same time class (`PREVIOUS_OWN_RATING_SQL`). We do not import the opponent's previous
     game, so the opponent's rating is the post-game rating minus the mirrored change:
     `opp_post + (own_post - own_pre)`. This is exact for a zero-sum update. Glicko's RD makes it
     approximate.
   - **Fallback:** an unrated game, or a Chess.com game with no earlier rated game, keeps the
     stored ratings.
   - **Where it applies:** the game's metrics (`player_rating`, `opponent_rating`,
     `expected_score`) and the repertoire's expected scores.
   - **Known limit:** a gap in the import can make the "previous" game an older one.
4. **Recompute without the engine.** `PLAYER_GAME_METRICS_VERSION` goes from 2 to 3, so every
   version-2 row is stale and no evidence. `mark_stale_metrics` marks those games
   (`games.profile_stale_at`). It runs at worker start, and the recurring difficulty sweep
   recomputes the marked games. `cli recompute-metrics` does the whole job at once with a
   cache-only analyser, so no engine ever starts. A game it cannot evaluate keeps its rows and its
   mark for the sweep. Each recompute rewrites four things from the stored evals:
   - `game_metrics`;
   - the openings and series rows;
   - `games.white_accuracy` / `black_accuracy`;
   - the account's `build-profile`, which rebuilds the profile and the KPI snapshots.
5. **Opening names stay as they are.** We label by position (EPD), as lichess-org/chess-openings
   and lila do. That is why 1.e4 Nf6 2.Nf3 is "Zukertort Opening: Lemberger Gambit": the dataset
   names that position only under 1.Nf3 Nf6 2.e4. This is Lichess's behaviour, so we keep it.

## Consequences

- On the audited data, the worst affected side (the `harmonic-floor` fixture) goes from 38.9 to 57.4, and the mean of the
  player's sides goes from 66.15 to 66.96.
- Phase accuracies drop to the game-accuracy scale. Area scores built from them need a band
  reference built at version 3. Reference observations written at version 2 are stale and are
  left out, like any other stale row.
- Analyse and recompute each do one more indexed read per game: the previous rating.
