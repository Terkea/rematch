# 0081. The peer benchmark compares with the pooled rate, in the player's window, without mirrors

Date: 2026-11-22. Status: accepted. Amends ADR 0080 (decisions 2 and 4). Bug hunt N9, verified
independently.

## Context

ADR 0080 compared the player with the opponent sides of their own games. An independent check of
the owner's rapid benchmark found three errors:

1. **A skewed reference.** Peer rate rows served the Beta-binomial fit's median and quartiles.
   With about one game per opponent and chances clustered in a few games (a missed mate comes
   in runs within one position), the fit reads the clustering as player-to-player spread and is
   U-shaped: missed mate showed 2.6% where the pooled like-for-like rate is 9.6% (9 of 99). The
   median also fed `confidence`, `excess` and so the priority.
2. **Mirrors.** In a game against the player, the opponent's W5 is 1 − the player's W6 (42 of 42
   games) and the opponent's W6 is 1 − the player's W5 (41 of 41); the opponent's W7.prep at 40% or
   less is the player's at 60% or more. Comparing them compares the player with themselves.
3. **Different periods.** The player's estimate counts 183 days with a 60-day half-life; the
   opponents counted every game in the band.

## Decision

1. **The compared value of a peer rate row is the pooled rate.** Each distinct opponent `i`
   with `g_i` games in the window contributes `(Σ w·k / g_i, Σ w·n / g_i)`, `w` being each
   game's recency weight: at most one game's worth of chances, scaled by recency, so a frequent
   opponent weighs as much as any other and a recent game more than an old one. The rate is
   `Σ_i k_i / Σ_i n_i`. It is stored as both `median` and `pooled_rate`, so the estimate's
   `confidence`, `excess`, `precise`, the timeline's states and the CoachingInput all use it (no
   change to `priority.rs`). `epl_per_event` is pooled the same way; `opps_per_game` is the
   unweighted mean of the opponents' chances per game.
2. **Its spread is its own uncertainty.** `p25/p40/p75` are the quantiles of the pooled rate
   over 2,000 resamples of the opponents with replacement (a cluster bootstrap by opponent,
   seeded with mulberry32 `0x00810081`, so a rebuild of the same sides gives the same rows),
   clamped to stay ordered around the pooled rate. A percentile against a peer rate row
   therefore says how far the player's rate sits from the peers' rate relative to how well that
   rate is known; it is not a rank among players, which one game per opponent cannot estimate.
   Wilson was rejected: it treats chances as independent, which the clustering breaks.
3. **Mirrors are not compared.** `is_peer_comparable` drops W5, W6 and W7.prep, and the base
   metrics built on the same episodes: conversion and resilience (each the other's mirror),
   the "out worse" rate and the eval after the opening (W7.prep's), and the eval trend (the
   player's, negated). Their rows show "Not compared". W4.missed, W4.allowed, W7.traps, W9a and
   W9b stay compared, as do accuracy, blunders, time use, missed punishments, endgame conversion
   and hold, losses on time and saveable resignations.
4. **The player's window.** Opponent sides older than 183 days before the build are left out
   (of every row and of the band's player and game counts), and rate rows weigh games with the
   player's 60-day half-life. Mean rows (accuracy, time per move) use the window, with one
   unweighted mean per opponent, as before.
5. **The web names it.** `account_benchmark_stat.n_chances` stores each rate row's raw chances
   in the window. A peer weakness row reads "Players you've faced, rated about 667–967: 9.6%
   (pooled over 65 of their chances)", followed by the bias sentence
   (`BENCHMARK_BIAS_TEXT`, unchanged). The detail page shows the same line in place of "Band
   median".
6. **The Lichess band path is unchanged** (`Weighting::Games`: the Beta-binomial median and
   quartiles, every game weight 1), in case `BENCHMARK_SOURCE=lichess-bands` is used again.

## Consequences

- The owner's rapid missed-mate benchmark moves from 2.6% to about 10% (7 of 65 chances, 25
  opponents, in the window), so a player rate near 10% is no longer flagged against a 2.6%
  reference.
- Rows for W5, W6 and W7.prep disappear from the stored benchmark at the next build.
- Fewer opponents fall in the window, so some rows may become low-sample and uncompared.

## Amendment 1 (2026-11-23): a rate row needs its chances; Both is not the colours' mean

Final QA on the owner's data (items A12 and A13).

1. **A12: a peer rate row needs the KPI's own minimum of chances.** `low_sample` counted
   opponents only (fewer than `LOW_SAMPLE_PLAYERS`, 10), so a rate row pooled over 10 chances
   was served. A peer rate row (`Weighting::Players`) is now also `low_sample` when its raw
   chances in the window (`n_chances`, the count the web label names) are below the KPI
   catalogue's `min_n` for the metric: the number of chances the player's own estimate needs
   before it counts as sufficient (`n0(k)` for a weakness: W4.missed, W4.allowed, W7.traps and
   W9b 40, W9a 30; base rates: blunders on a low clock 40, missed punishments 30, time trouble
   and losses on time 20, endgame conversion and hold 15). The player is never compared on less
   evidence than the player's own number needs. Mean rows keep the player rule, and the Lichess
   band path (`Weighting::Games`) is unchanged. On the owner's rapid data, W4.missed White (31
   chances) and Black (34) and endgame conversion Both (10) become uncompared at the next build.
2. **A13: Both is not the chance-weighted mean of White and Black, by design.** The owner's rapid
   `losses_on_time_rate` reads Both 0.16%, White 0.63%, Black 0%. The only loss on time in the
   window is by one opponent faced 8 times, 4 with each colour. Each opponent weighs one game's
   worth in each pool it is in (decision 1), so that event is 1/4 of a unit in White and 1/8 in
   Both, and Both holds about twice White's units: Both ≈ White / 4, not White / 2. Re-pooling the
   stored rows with SQL reproduced it (Both 0.0015, White 0.0059). The two pools use the same
   denominators (recency-weighted chances per opponent game) and the same decay; when no opponent
   is in both colour pools, Both is exactly the unit-weighted mean of the colours
   (`both_weighs_an_opponent_once_however_many_colours_they_had`). An opponent is one player of
   the Both pool however many colours they had, and one of each colour's pool: that is the rule
   that stops a frequent opponent dominating, so it stays. The colour rows are the colour's own
   comparison (ADR 0082), never a share of the Both one.
