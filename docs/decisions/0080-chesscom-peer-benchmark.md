# 0080. A chess.com peer benchmark from the opponents you have faced

Date: 2026-11-22. Status: accepted (owner: "leave lichess out of this, prioritise only
chess.com"). Supersedes, for the default flow, the Lichess-dump band reference of ADR-R2-11 and
ADR-R2-23 as the benchmark of weakness estimates, timeline states and area percentiles. Builds
on ADR 0078 (uncompared weaknesses).

## Context

Every comparison (a weakness's `excess` and `P_imp`, its state other than "not compared", the
area percentiles) needs a reference group. R2 built it from a streamed Lichess dump: a sampled
band of Lichess players, analysed by our engine, on the Lichess scale with a provisional
chess.com offset. The owner plays on chess.com only and wants nothing downloaded or streamed from
Lichess, so on the owner's stack nothing was ever compared.

Every analysed game already has main evals for **both** colours. The owner's opponents are
chess.com players rated close to the owner (a mean gap of about 50 points; 516 games, 185+
distinct opponents). Their side of each game is a reference observation that costs no download
and no engine time, on the same rating scale as the owner.

## Decision

1. **Observations.** For every analysed game, build-profile computes the opponent side's
   `PlayerGameMetrics` with the same extractors as reference sides (`compute_game_features`), in
   the opponent's colour (`opponent_context`: colour flipped, the player's pre-game ratings
   swapped, the score mirrored, the opponent's game accuracy). Inputs are the stored PGN and the
   cached main evals (`DbEvalCache` with `NoEngine`): a game with an uncached position is skipped
   and counted, never evaluated. The rows are cached in `opponent_game_metrics`, keyed to the
   player's `game_metrics.created_at`, so only new or re-analysed games are recomputed.
2. **Only what an unenriched side measures exactly is compared** (`is_peer_comparable`). The
   opponent side has no enrichment (refutation, threat and best lines, tablebase probes, Maia).
   Several detectors would read a missing line as a non-event: W1 still counts non-decisive
   moves as opportunities, so its rate would be biased towards zero, and the per-game counts that
   skip unknown lines (one-move blunders, tactics allowed) would undercount. So these are never
   produced: W1, W2, W3, W8, W9c, W10, W11, every motif row, one-move blunders, tactics, sharp
   errors, quiet loss, critical time ratio and tablebase errors stay "not compared". Compared:
   W4.missed, W4.allowed, W5, W6, W7.traps, W7.prep, W9a, W9b, accuracy (overall and by phase),
   blunders per game, low-clock blunder rate, conversion, resilience, missed punishments, the eval
   trend, time trouble, losses on time, time per move, the opening's eval and "out worse" rate,
   book exit, time in the opening, endgame conversion and hold, saveable resignations. Score vs
   expected is left out as well: against the player it is the player's own result mirrored.
   Session metrics have no sessions for opponents and so no row.
3. **The band** is the player's latest pre-game rating in the time class ± 150, on chess.com's
   own scale (no offset). It is reported as "Compared with players you've faced on chess.com,
   rated about 650–950 (12 players)". Opponents outside the window are left out.
4. **Players are distinct opponents** (case-insensitive usernames), and each weighs once: their
   sums are divided by their games before the pooled rate, the EPL per event, the opportunities
   per game and the slope (one point per opponent) are taken. The Beta-binomial fit and the
   mean quantiles already take one value per player. So one opponent faced 40 times counts as
   much as one faced once.
5. **Minimum sample** is the band reference's rule: a row with fewer than 10 opponents with
   evidence is `low_sample`, stored and never served. No fallback to another band or source.
6. **The bias is stated** wherever the benchmark is named: "Only their games against you count,
   so it is a rough guide, not a rating-band average." Opponents are measured only in games
   against this player (who may be better or worse than their average opponent), and their
   weaknesses are partly the player's strengths.
7. **Storage.** build-profile writes `account_benchmark` (source, centre, window, players,
   games) and `account_benchmark_stat` (the rows, as `band_reference`) in its transaction. The
   CoachingInput builder reads the served rows from there, so the AI input and the stored
   estimates always compare with the same thing. The web reads the band for its label.
8. **The Lichess path is opt-in.** `BENCHMARK_SOURCE=lichess-bands` makes build-profile read the
   active Lichess band reference as before (stored with source `lichess_bands`). The default is
   `peers`. `lichess-db build --band-reference` and its code stay (the explorer and its tests
   use the same crate), but nothing in the default flow depends on them.
9. **Enrichment with more opponent games** (fetching those opponents' other chess.com games
   through the public API and analysing them) is not built. It would cost engine time and
   rate-limited API calls; the faced-opponents band is enough to compare. It can come later
   behind a flag, default off.

## Consequences

- The owner's estimates get a non-null `excess` and `P_imp` and real states for the compared
  keys once 10 distinct opponents in the window have evidence for them. Bullet, with fewer
  games, may stay uncompared.
- The first build of an account reads every game's cached evals once (about a second per 50
  games); later builds only new games.
- The benchmark is specific to each account and time class, and moves with the player's rating.
- Detectors that need enrichment stay "not compared" for every chess.com player. Enriching the
  opponent side (more engine time per game) would lift that; it is a separate decision.
