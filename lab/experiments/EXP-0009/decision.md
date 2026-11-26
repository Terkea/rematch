# EXP-0009: decision

- Decision (section 9, outcome 3): **need more data; the trainability rows stay.** No weakness key's fall from August to September came before a rise in October's rating, four of the eight were shown below the minimum effect (six were needed), and the positive control failed: a rise in accuracy did not come before climbing either, although its interval was narrow enough to see a link of the minimum size (99.44% [−1.49, 1.24] against ±3.63). What failed is the control's premise, not the precision. The likeliest reason is a flaw in the primary's design (below, "Why the control failed"), which the follow-up must fix. We have no data-driven teaching order yet. The one follow-up the rule allows is EXP-0029 (Next, item 1).
- Stage 2 (our own analysis at 1 million nodes, for the nine keys that need the enrichment) does not run: at this design the median key would need about 5,240 players × 80 games, about 4,560 CPU-hours, 228 times its ceiling of 20 (`power.json`). That price is for a design whose control failed, so it is not filed as an issue of its own; it is named here with its cost for the owner, and EXP-0029 prices it again (section 11 of the pre-registration records the deviation from section 0's "filed").
- Algorithm: `algo-4.0.0+51733d23895f`. Results: `results.json` (commit 4057a7cb), two runs byte-identical (sha256 `82d941f1ca6db506667e25bcd90aad407256d139f2d07feb8d1d703ea0492186`). Figure: `figures/primary.png`.
- Cohort (ADR 0114): the Lichess standard rated dumps of 2026-07 to 2026-10 (CC0, many thanks to Lichess and its players), 366.2 million games in all, each streamed once and verified against Lichess's `sha256sums.txt`; the players followed by `sha256(salt ‖ lower(name))` alone, no name stored. Confirmatory slice: hash fraction in [0, 0.3), from the pilot's power analysis. Of 86,819 players with 20 or more rapid games in July, 18,968 had 10 or more analysed rapid games in both August and September, 17,138 ended August below 2000, 17,136 kept 10 observed games in both months (B0 554, B1 4,009, B2 7,514, B3 5,059), and 16,204 of those (94.6%) played 5 or more rapid games in October (16,206 of the 17,138; two of them had too few observed games). 877,072 games read and sent to observe, 876,947 observed, 125 failed (an evaluation list that did not match the plies). The next month's rating change has an SD of 53.8 Elo.
- Compute: 877,072 `observe_games` of a budget of 1,200,000 (73%), 1,000,000 `engine_nodes` (100%, the start position once); one run took 48 minutes on 8 workers, about 1.5 CPU-hours, most of it parsing 20 GB of games in one Python thread. The pilot took 18 minutes. The four dumps took about 3.3 hours to stream in parallel (about 118 GB at 9 to 12 MB/s), about 3 CPU-hours; July and October were streamed a second time by a mistake in the lane's script, which changed nothing (the same verified digests and files). No AI tokens.

## The primary: does a fall in a rate come before climbing?

Next month's rating change (October), per observed SD of the improvement from August to September, adjusted for the terms of section 8; Holm across nine. The minimum effect, 14 Elo per true SD, is 14 × √reliability per observed SD.

| Quantity | Players | Elo per observed SD | 95% | Simultaneous (99.44%) | Reliability | Minimum | Reads |
|---|---|---|---|---|---|---|---|
| W4.allowed | 16,204 | −1.18 | [−2.23, −0.14] | [−2.66, 0.29] | 0.051 | 3.17 | below the minimum effect |
| W4.missed | 9,653 | 0.75 | [−0.76, 2.26] | [−1.39, 2.88] | 0.035 | 2.64 | undetermined |
| W5 | 10,416 | −2.94 | [−4.34, −1.54] | [−4.91, −0.96] | 0.087 | 4.13 | precedes falling (Holm p 0.0003) |
| W6 | 12,099 | −1.03 | [−2.19, 0.12] | [−2.66, 0.60] | 0.056 | 3.32 | below the minimum effect |
| W7.prep | 16,204 | 0.66 | [−0.27, 1.59] | [−0.66, 1.97] | 0.019 | 1.95 | undetermined |
| W7.traps | 16,204 | 0.04 | [−0.92, 0.99] | [−1.31, 1.39] | 0.048 | 3.08 | below the minimum effect |
| W9a | 16,204 | −0.02 | [−0.84, 0.80] | [−1.18, 1.14] | 0.131 | 5.07 | below the minimum effect |
| W9b | 2,094 | −0.32 | [−3.25, 2.60] | [−4.47, 3.82] | 0.040 | 2.79 | undetermined |
| accuracy (control) | 16,204 | −0.12 | [−1.09, 0.84] | [−1.49, 1.24] | 0.067 | 3.63 | below the minimum effect: the control fails |

The one Holm-significant link runs the wrong way: players whose W5 rate (not winning won games) fell gained less in October. The "adjusted" sensitivity, which adds September's own rating change, takes it to −0.45 [−1.83, 0.92]. The leading explanation is regression to the mean rather than harm: converting more won games in September raised September's rating, and October drifted back. The results do not store the adjusted model's coefficient on September's change, so that link is inferred from the shift, not read directly.

## Why the control failed

The outcome is R_end(October) − R_end(September), and R_end(September) carries September's luck, which comes from the same games that produce the improvement Δq. Within September, every quantity but W9a moves with the rating (below), so the primary also picks up October's drift back from September's luck, which pulls every β towards negative; W5, defined by results, is the extreme case. With September's change in the model, accuracy reads +3.0 [2.1, 4.0]. A second, untested reading is that rating catch-up mostly happens within the month; the adjusted +3.0 says it is not finished there, and the level family's outcome runs from August to October, so it cannot separate the two. Either way, longer windows alone keep the flaw: EXP-0029 must register an outcome baseline not built from the exposure games (a gap month between them, or month-mean ratings instead of end ratings), and justify it before it reads new data.

## What the rest shows (secondaries, none decides)

- Within the same month, everything but W9a moves with the rating (the issue's climbers against stayers): per observed SD of improvement, accuracy +14.1 Elo [13.1, 15.2], W5 +12.5, W6 +8.5, W7.traps +7.0, W4.allowed +6.1, W7.prep +5.6, W4.missed +5.2; W9a +0.8 [−0.01, 1.65]. The rating change and the rates come from the same games, so this is descriptive for every key, and only that for W5, W6 and accuracy, which are partly defined by results.
- With September's rating change in the model ("adjusted"), accuracy +3.0 [2.1, 4.0], W7.prep +1.9 [1.0, 2.8] and W7.traps +1.5 [0.6, 2.5] read "precedes climbing". This model adjusts for a consequence of the improvement (registered as a sensitivity for that reason), so it cannot decide; it is the main evidence for the design flaw above.
- The August level predicts the change to October: per SD of a better level, accuracy +6.2 Elo [4.6, 7.9], W7.prep +5.9 [4.6, 7.1], W7.traps +4.4 [2.9, 6.0], W4.missed +2.5 [1.0, 4.1] and W5 +2.1 [0.5, 3.6] (Holm); W4.allowed, W6 and W9a below the minimum. The level is far more reliable than the change (0.36 to 0.79 for most keys). This is rating catch-up: a player who plays better than their rating rises towards it, which is not evidence that teaching the key helps.
- Attrition: 94.6% stayed active; no change in any quantity moved the chance of staying by more than 0.6 points per SD, and the inverse-probability-weighted primary matches the unweighted one (W5 −2.85 [−4.23, −1.47], accuracy 0.15 [−0.86, 1.16]).
- By band (exploratory): the W5 sign holds in B1 to B3 (−2.4 to −3.8); W7.traps is +6.3 [0.8, 11.8] in B0 (554 players) and near 0 above.
- Blunder rate (exploratory): +0.97 [−0.02, 1.96].

## What this does not show

- No causal claim, either way: these are anonymised Lichess players who ask for server analyses, one month apart; the result is no candidates to teach, not evidence that teaching any key is useless.
- Confounders the design does not remove: the luck shared by the exposure games and the outcome's baseline (above); which games a player chooses to have analysed, which may lean towards losses and change from month to month beyond the analysed-share covariate; rating deviation, since a provisional or returning account's rating moves more and its rates may too; and second accounts and sandbagging, which nothing here can see.
- The change in a rate over 10 to 40 games is mostly binomial noise (reliability 0.02 to 0.13). The reads use the point estimate of each reliability; with about 16,000 players its standard error is near 0.008, so W7.traps' 0.048 could be 0.02 to 0.08 and its minimum 2 to 4 Elo per observed SD. The four "below the minimum effect" reads rest on those points. W7.prep missed that read by 0.02 Elo (upper end 1.97 against 1.95). By Spearman-Brown, three times the games lifts a reliability of 0.05 only to about 0.14, so longer windows help less than they seem to; EXP-0029 must bootstrap the reliability or read its lower bound.
- W7.prep is the self-hosted value (the opening ends at ply 20 without an explorer). W9b had only 2,094 players with five low-clock chances in both months.
- The nine keys that need the enrichment, W1 "Leaving pieces hanging" and the tactics among them, are not measured; at this design they would cost about 4,560 CPU-hours.

## Deviations and process

- None from sections 0 to 10 as frozen at commit 4057a7cb. Changes between registration and the run are listed in `review.md`, part one: the critic's fixes, the input checksums, two memory fixes that change no result (the first pilot attempt stopped at 5.5 GB before it observed anything).
- The Docker host lost its E: drive share during the lane; the pilot and both runs read the cohort from a copy in a Docker volume, checked by the same `inputs.sha256` (every file matched).
- `games_with_an_opponent_in_the_other_slice` (91,366) counts games whose opponent's pseudonym falls in the pilot's hash range, whether or not that opponent is in the pilot's cohort, so it overstates the shared games.

## Next

1. EXP-0029: EXP-0009's follow-up, `Follows: EXP-0009`: an outcome baseline not built from the exposure games (a gap month or month-mean ratings), three-month windows on months disjoint from July to October 2026 (2025-10 to 2026-06), the same salt with EXP-0009's whole cohort excluded, its power from this run's reliabilities with their uncertainty, and its value case first, since longer windows lift reliability less than they seem to. A second "need more data" keeps the rows.
2. EXP-0004: the leakage-free Tier 2 design on real players can reuse `lichess-db cohort` (ADR 0114) for month t against t+1, with its own salt.
