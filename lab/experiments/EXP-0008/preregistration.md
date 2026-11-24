# EXP-0008: is 300,000 nodes enough? Depth against judgements and cost

- Status: pre-registered
- Issue: #103
- Author: the maintainer, with Claude (lane `exp/0008-depth`)
- Kind: measurement (one version, at five node counts against a deep reference)
- Follows: none
- Registered: 2026-11-24. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: `ENGINE_NODES` (300,000, `backend/crates/engine/src/types.rs`, protocol `sf19-n300k-v1`), the largest compute cost of the pipeline, and with it the node count of EXP-0009's cohort.
- If the result is that 100,000 nodes is within the tolerance of 300,000 (and 1 million is not better than 300,000 by more than it), we will propose a 100,000-node protocol, a third of the engine time a game, as a major algorithm version; if 1 million nodes is better than 300,000 by more than the tolerance, we will propose 1 million (or 3 million, by the same test one step up) and re-cost the pipeline; if 100,000 is shown worse than 300,000 by more than the tolerance and 1 million is not better, 300,000 stays and the curve is published; otherwise 300,000 stays and the experiment needs more data.
- Expected cost: the candidates (50,000, 100,000, 300,000 and 1 million nodes) on every position of the sampled games, the 3 million nodes candidate only if the step up needs it, and the 30-million-node reference only on the moves where the candidates disagree and on a seeded sample of the rest; at the measured 1.65 million nodes a second a thread, the number is set by section 6 and capped at 12 CPU-hours; no AI tokens; about a day of maintainer time.
- Value score: impact 3 × uncertainty 2 ÷ cost 2 = 3. It runs early despite the score because EXP-0009's cost depends on it.
- Cheaper alternative? Partly. Lichess's own `[%eval]` comments cost no engine time, but they come from another build and depth, so they show whether we agree with Lichess, not whether 300,000 nodes is enough; they are the secondary that replaces a separate experiment on judgements against Lichess's (the judgement rule is lila's, so a disagreement can only come from the evaluations). No cached evaluation applies: `position_evals` holds 300,000-node evaluations of real chess.com players' games, which this experiment does not use (section 7), and no other experiment has searched these positions. The design spends the deep searches only where they can move the answer (section 7).
- Compute budget: set from section 6 before the run, in nodes the design asks for: `candidate_nodes`, `reference_nodes` and `recheck_nodes` (section 6 gives the numbers); cap 150%

## 1. Question

Does a 300,000-node search give a player the same move labels (inaccuracy, mistake, blunder) as a search a hundred times deeper, would 100,000 nodes give labels as good at a third of the cost, and would 1 million give better ones? A 1500 player who sees "blunder" on 12...Nd4 should see it because the move loses, not because the search stopped early: if a deeper search calls the same move a mistake, the player is taught the wrong thing about that move, and their blunder rate, which the profile ranks (`backend/crates/profile-core/src/weakness/detectors.rs`), counts it.

The judgement rule is lila's (`backend/crates/chess-core/src/judgement.rs`: a fall in winning chances of 0.3, 0.2 or 0.1, with the mate rules), so labels move only when the evaluations move. Game accuracy (`game_accuracy.rs`) reads the same evaluations. The ranked weaknesses are not measured here (section 4).

## 2. Hypotheses

A measurement study with three branches (section 9).

- H0 (step down): 100,000 nodes disagrees with the reference on more than 0.01 of moves more than 300,000 does.
- H1 (step down): it disagrees on less than 0.01 of moves more (it is within the tolerance).
- H0 (step up): 1 million nodes disagrees with the reference on at most 0.01 of moves fewer than 300,000 does.
- H1 (step up): it disagrees on more than 0.01 of moves fewer.

Rival explanations that would give the same numbers, and how each is handled:

- The reference is a search too. A 30-million-node search is not the truth, and its own errors correlate more with 1 million nodes than with 100,000, because the deeper candidate sees more of what it sees. This favours the step up and works against the step down; the decision accepts that bias in the cautious direction. Where 5 pieces or fewer remain, the Syzygy tables make the reference exact on the result.
- Search noise is not depth. A node-limited search at one thread is deterministic (ADR 0104), so two searches at the same count never differ; a label that changes between 100,000 and 300,000 changes because of the depth.
- Analysed games are not all games. The sample is drawn from games whose players asked Lichess for a server analysis (they carry `[%eval]`), which may be sharper or more decisive than the rest. The secondary needs those evaluations; the primary does not, and the claim is stated for this population.
- Lichess games are not chess.com games. The players are Lichess's rapid players rated 800 to 2199 on Lichess; the engine and the rule do not depend on the site, but the mix of positions might.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-3.0.0+554036a3b27c` | – |

From `cli algo show` on the cli built from this branch. The bench crate gains `bench::depth` and the cli gains `cli lab positions|evaluate|judge`; neither crate is in the manifest, and the id is unchanged. The node counts are measured on this version's code; a node count other than 300,000 is the protocol a successor version would ship, not a version that exists.

## 4. Primary metric and ground truth

- Primary metric (exactly one): the difference in label disagreement with the reference, D(N) − D(300,000), where D(N) is the share of moves whose judgement (none, inaccuracy, mistake or blunder) at N nodes differs from the reference's, over every move of every sampled game, with the game as the cluster. The decision reads two contrasts of it: D(100,000) − D(300,000) (the step down) and D(300,000) − D(1 million) (the step up); D(50,000) − D(300,000) and D(1 million) − D(3 million) are read only after them, in a fixed sequence (section 9).
- Ground truth: a single-thread **Stockfish 19** search at 30 million nodes, 128 MB of hash and the Syzygy 3-4-5 tables, `ucinewgame` before every search, scored by the production code exactly as a 300,000-node search is (`evaluate_game`, `build_move_records`).
- Secondary metrics:
  - confirmatory: game accuracy, the difference of each player's accuracy in each game at N from 300,000's (the mean difference, the mean absolute difference, the share of players' games that move by a point or more), with 95% intervals; no reference is needed;
  - confirmatory: agreement with Lichess's labels, the share of moves whose judgement on Lichess's own `[%eval]` (put where production puts a Lichess game's) differs from the judgement at 300,000 nodes, with its 95% interval;
  - exploratory: D(N) itself for each N and for Lichess's evaluations, best-move agreement with the reference, and the mean absolute difference in the mover's win% before the move, from the reference's, each from the stratified sample of section 7 with a seeded bootstrap interval;
  - exploratory: the step contrasts within each rating band; the labels a player sees per game at each N; the nodes each N costs a game.
- Not measured: the ranked weaknesses. A ranking needs each player's whole history analysed at each node count, the enrichment's MultiPV and threat searches included, against a reference no one can afford for whole histories. The labels and the win% losses are what the profile's judgement series count; the follow-up that measures the ranking at the chosen count is under "Next".

## 5. The minimum effect that matters

- Minimum effect: a tolerance of 0.01 of moves. The step down is within it when 100,000 nodes disagrees with the reference on less than 0.01 of moves more than 300,000; the step up is worth it when 1 million disagrees on more than 0.01 fewer. Power is set at half the tolerance from its boundary (0.005 and 0.015).
- Why this size: one label in a hundred moves is, for a player whose games run to about 35 moves, one changed label in one game in three: the smallest change a player would notice in a game's list of inaccuracies, mistakes and blunders. Below it the node count is a matter of cost alone; a third of the engine time is worth a change a player would not see, and three times the engine time is not worth one.

## 6. Power analysis

To be completed from the pilot (section 7), before the confirmatory sample is drawn.

- Method: a simulation of the section 9 rule (`analysis.py power`): n games drawn with replacement from the pilot's, each game's two contrast totals shifted so the population contrasts are the scenario's and their spread around it scaled by k, 2,000 replicates a size and scenario, seed names `power/simulation/<variance>/<n>/<scenario>`.
- Scenarios: "100k is enough" (step down 0.005, step up 0.005), "300k falls short" (step up 0.015), "300k is right" (step down 0.015, step up 0.005); each must reach its own outcome with probability at least 0.80. Two boundary scenarios (step down 0.01; step up 0.01) give the rate of a wrong move.
- Variance: the pilot's (k = 1) and its 80% upper bound from n − 1 degrees of freedom (k = √((n − 1)/χ²₀.₂₀,ₙ₋₁)); the size is the smallest on the grid 40, 60, 80, 100, 120, 140, 160, 200, 240, 300, 400 that reaches 0.80 in all three scenarios with the upper bound.
- The ceiling: if that size costs more than 12 CPU-hours at the pilot's measured rate, the confirmatory sample is the largest grid size within 12 CPU-hours, and this section states the power it reaches.

## 7. Design

- Unit of analysis: the game. Its moves are nested in it and never counted as independent. No player is in two sampled games (the pilot's included), so the games are independent.
- Population: the rated rapid games (Event "Rated Rapid game", no tournaments, standard chess from the start position) of the extract below whose movetext carries Lichess's `[%eval]` on every ply (the final ply may lack one), with a mean of the two ratings in [800, 2200), at least 20 plies, and the termination "Normal" or "Time forfeit".
- Sample: the population in a seeded order (`sample/order`), keeping a game only when neither player is in a game kept before it; the first 20 kept are the pilot, the next n (section 6) the confirmatory sample (`sample-pilot.csv`, `sample.csv`). Simple random sampling, so every estimate is over the population.
- Settings held fixed: one thread, `ucinewgame` before every search, `go nodes N`, MultiPV 1, the last complete line, exactly as `evaluate_game` searches (`cli lab evaluate`, checked against the production engine pool by `backend/crates/cli/tests/lab_depth.rs`); the candidates with production's 16 MB of hash and no tables; the reference as in section 4. The **Stockfish 19** build is the manifest's.
- What is searched: every position the production analysis searches (`cli lab positions`), at 50,000, 100,000, 300,000 and 1 million nodes. The reference, on the moves where the four disagree (a move where two settings agree adds nothing to their contrast whatever the reference says, so every contrast of section 4 is exact) and on a seeded sample of 150 of the moves where they agree (`concordant/confirmatory`), which the exploratory absolute measures need. 3 million nodes on every position only when section 9's step up is shown, then the reference on the moves it adds to the disagreement. A move is judged on the reference when both of its positions are searched at the reference; a search of another position never enters its label.
- Seeds: `sample/order`, `concordant/pilot`, `concordant/confirmatory`, `recheck/<setting>` for each setting, `bootstrap/<metric>/<setting>`, `power/simulation/<variance>/<n>/<scenario>` (`rematch_lab.experiments.seed`); no other source of randomness.
- Development and confirmatory data: the pilot (20 games) is development data: it sets the power analysis and is excluded from the confirmatory sample, games and players both. The tolerance, the metric, the reference and the rule (sections 2, 4, 5 and 9) are committed before the pilot runs.
- The cache: every search is kept in the data directory (`exp08-evals/<setting>-<Stockfish sha256>.jsonl`), and a second run reads it. The run re-searches 12 seeded positions of every setting it used without the cache (`recheck/<setting>`) and stops unless each matches its cached search in score, best move, depth and nodes; results.json records it.
- Data: the first 64 MiB of the Lichess standard rated database for October 2026, `https://database.lichess.org/standard/lichess_db_standard_rated_2026-10.pgn.zst` (Last-Modified 2026-11-02 01:23:06 GMT, 30,982,323,881 bytes in all), fetched 2026-11-24 07:50 UTC with an HTTP range request for bytes 0 to 67,108,863, sha256 `3f7cd8fc154b8ddea83316811b18decfce451dcf493a7b3dbf92a4db3277ff4d`; decompressed with zstd 1.5.6 to the last complete game, 481,032,464 bytes and 205,631 games, sha256 `ee83071caabf515c7df5ebb03b8b6246be1a887689135c9f5e0d952409e21604` (`data/lichess-games/lichess_db_standard_rated_2026-10.head.pgn`, git-ignored, the external input of `inputs.sha256`). The database is CC0; many thanks to Lichess and its players. Only game ids, players' names, mean ratings and ply counts are committed (`population.csv` and the samples). No real chess.com player's games.
- Leakage controls: the pilot and the confirmatory sample share no game and no player, and the run checks it.
- Reuse: none from earlier work (section 0); within the experiment, every search once, through the cache.

## 8. Analysis plan

- Model or test: no model. Each contrast is a ratio over games, R = Σ_g E_g / Σ_g M_g, with E_g the game's sum over its moves of (d_A − d_B), d = 1 when the setting's label is not the reference's, and M_g its moves. Its variance is the linearised one, n/(n − 1) Σ_g (E_g − R·M_g)² / (Σ_g M_g)², and its interval R ± t·SE with n − 1 degrees of freedom.
- Effect size: each contrast with its 80% and 95% intervals; the decision reads the 95% interval.
- Multiple comparisons: each wrong move away from 300,000 needs one end of one 95% interval to fall on the wrong side of the tolerance, a one-sided 0.025 error, so a wrong step up or a wrong step down has at most 0.025 chance and either at most 0.05. 50,000 is tested only after 100,000 passes, and 3 million only after 1 million passes, a fixed sequence, which spends no more. The secondaries are reported with uncorrected 95% intervals and decide nothing.
- Diagnostics: no fitted model. Section 6 reports the simulated rates of a wrong move at the boundaries.
- Exclusions and missing data: none after sampling. A game the production code cannot parse or judge stops the run (it passed `cli lab positions` when the population was built). A run that spans more than one `algorithm_id`, or a re-searched position that differs from its cached search, stops; a stopped run is a failed result.

## 9. Decision rule

Measurement at several node counts, with four outcomes read on the 95% intervals of section 8, in this order:

1. **300,000 falls short:** the lower end of D(300,000) − D(1 million) is above 0.01. The setting is 1 million, or 3 million when the lower end of D(1 million) − D(3 million) is also above 0.01. We propose that count as a major algorithm version.
2. **100,000 is enough:** the upper end of D(300,000) − D(1 million) is below 0.01 and the upper end of D(100,000) − D(300,000) is below 0.01. The setting is 100,000, or 50,000 when the upper end of D(50,000) − D(300,000) is also below 0.01. We propose that count as a major algorithm version.
3. **300,000 is right:** the upper end of D(300,000) − D(1 million) is below 0.01 and the lower end of D(100,000) − D(300,000) is above 0.01. 300,000 stays; the curve is published.
4. **Need more data:** anything else. 300,000 stays; at most one follow-up, registered with `Follows:` this experiment, its own power analysis and the same rule on the pooled games. A second "need more data" keeps 300,000.

This departs from ADR 0106's measurement rule (one interval against a band): the decision is a choice among node counts, so each move away from production has its own test against the tolerance, and production is kept unless a move is shown. EXP-0009 uses the setting this rule gives.

### Stopping rules

- Early stopping: none. The candidates are cheap; the deep searches are spent only on the moves that can move a contrast, which is where an interim look would save.
- Futility: none, for the same reason.
- Budget: the run aborts at the cap of section 0, before searching, since the budget counts the design's nodes first; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0008/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`; its `sample`, `pilot` and `power` stages run before registration, in that order)
- Analysis code sha256: to be stated at registration.

## 11. Deviations

None.
