# EXP-0009: what actually helps people climb

- Status: pre-registered
- Issue: #104
- Author: the maintainer, with Claude (lane `exp/0009-what-helps`)
- Kind: measurement (one version, `algo-4.0.0`, on an observational cohort)
- Follows: none
- Registered: 2026-11-24. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: the teaching order. `trainability` (`backend/crates/profile-core/src/constants.rs`, rows per band from `docs/research/improvement-engine.md` section 6.4) multiplies every weakness's priority (`stats/priority.rs`), and it comes from research notes, not from players who improved. With it, whether stage 2 of issue #104 (our own analysis at 1 million nodes, for the nine keys that need the enrichment) is worth its engine time.
- If the result is that a weakness whose reduction precedes climbing is one the trainability rows rank low, or that a key the rows rank high shows no such link, we will propose new rows in a comparison experiment of their own on new months (then a major algorithm version, in its own lane); if no key's reduction precedes climbing, at least six of the eight are shown below the minimum effect and the positive control holds, we will keep the rows and say plainly on the site that we have no data-driven teaching order yet; otherwise one follow-up with more months. Stage 2 runs inside this experiment only if its powered size costs at most 20 CPU-hours at 1 million nodes; otherwise it is filed with its cost for the owner to decide.
- Expected cost: four monthly dumps streamed once each (2026-07 to 2026-10, about 118 GB in all, about 3 hours of wall time at the mirror's measured 12 MB/s over parallel connections, about 2 CPU-hours to decompress and parse); `cli lab observe` on at most 1.2 million games, about 1 CPU-hour (360 games a second a process, measured on the October extract); one search at 1 million nodes; no AI tokens; about two days of maintainer time, including the cohort command and its ADR (0114).
- Value score: impact 3 × uncertainty 3 ÷ cost 2 = 4.5
- Cheaper alternative? Partly, and it is this design: stage 1 uses Lichess's own `[%eval]` comments, so it spends no engine time, and it sizes stage 2. Its limit is that the players are those who ask Lichess for a server analysis, and only eight of the seventeen rankable keys can be computed without the enrichment; that is why stage 2 exists and why this stage can change the rows only through a confirmatory comparison of its own. The research notes behind the rows are the cheapest answer and are what we use today; this is what checks them. No cached run applies: no experiment has followed players across months.
- Compute budget: 1,200,000 `observe_games` (games sent to `cli lab observe`) and 1,000,000 `engine_nodes` (the start position once); the streaming is outside `analysis.py` and is reported in `decision.md`; cap 150%

## 1. Question

Among anonymised Lichess rapid players in the audience range (rated below 2000, anyone who knows the pieces), does a fall in one of the rates Rematch measures come before a rise in rating? Concretely: a 1400 player whose share of games with an early decisive error (W7.traps, "Early blunders") falls from August to September; does that player gain more rating in October than an otherwise similar player whose rate did not fall? And, as secondaries, does the same hold within the same month (the issue's climbers against stayers), and does the level of a rate in August predict the rating change to October?

These are candidates to teach, never causes: the learning experiments (v0.6) test whether teaching them helps.

## 2. Hypotheses

For each quantity q of section 4, β_q is the expected next-month rating change, in Elo, per standard deviation of improvement in q from P1 to P2, adjusted as in section 8.

- H0: β_q = 0 for every q.
- H1: β_q ≥ 14 Elo for at least one of the eight weakness keys (a reduction precedes climbing).

Rival explanations that would give the same numbers, and how each is handled:

- Regression to the mean in the rating: a player picked after a dip rises. The outcome's starting rating, the change during P2, the change from P0 to P1 and the change within P0 are covariates; selection reads no rating change.
- Regression to the mean in the rate: a high rate in P1 falls in P2 by chance. The P1 level is a covariate of every change model.
- Rating catch-up: the rating lags the skill, so a player who has got better rises over the next weeks whatever caused it. This is not separable from "the reduction precedes climbing" and is the main reason no causal claim is made; the positive control (accuracy) is expected to show it.
- New or returning accounts: a minimum history of 20 rapid games in P0, the band reference's activity rule (ADR-R2-11).
- Reverse causation: a higher rating brings stronger opponents, who punish more. The primary is lagged (the change in P1 to P2 against the rating change in P3), and the change in the mean opponent gap is a covariate.
- Time-control switching: the rapid share of the player's games in P2 and the rapid games in each month are covariates.
- Rating inflation or deflation: every player is in the same months, so a pool-wide drift is in the intercept.
- Who asks for analysis: only games with a server analysis carry `[%eval]`, so the players are those who ask for it, and their analysed games may not be their typical ones. The claim is stated for this population.
- Attrition: players who stop playing rapid in P3 have no outcome. Their share is reported, and whether the change in each quantity predicts staying (exploratory).
- Noise in the rates: rates from 10 to 40 games are noisy, which shrinks every estimate towards 0; this works against H1, and against "below the minimum effect" not at all, so section 9 needs the positive control before it reads a null as small.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-4.0.0+51733d23895f` | – |

From `cli algo show` on the cli built from this branch. The bench crate gains `bench::observe`, the cli `cli lab observe` and `lichess-db` its `cohort` command; none is in the manifest, and the id is unchanged (`node scripts/algorithm/check.mjs`).

## 4. Primary metric and ground truth

- Primary metric (exactly one family, Holm across its ten members): β_q for each of the ten quantities below, each read on its own regression.
- The quantities, per player and feature month, from `cli lab observe` (the production `evaluate_game` on Lichess's evaluations, `build_move_records`, `game_accuracy`, `compute_game_features` with no enrichment and no opening explorer, `observe_game`):
  - the eight rankable keys whose detectors read only the evaluations, the clocks, the board and the result (`bench::observe::EVAL_ONLY_KEYS`): W4.missed, W4.allowed, W5, W6, W7.traps, W7.prep, W9a and W9b, each as events over opportunities summed over the month's games (missing below 5 opportunities);
  - the blunder rate (blunders over the player's moves) and the mean game accuracy, the positive control.
- Ground truth: the players' Lichess rapid ratings in the monthly dumps (CC0): the mean of the ratings after the last five rapid games of a month (its end) and before the first five (its start).
- Not measured, and why: W1, W2, W3.shallow, W3.deep, W8.basics, W8.advanced, W9c, W10 and W11 need the enrichment (MultiPV lines, the refutation and threat probes, the tablebase probe, Maia); without it W1 would count opportunities and never an event. They are stage 2. The ranked weaknesses need every key and the band reference; also stage 2. "Which fell first against the rating's change point" needs more than two feature months; not measured.
- Secondary metrics:
  - confirmatory: the contemporaneous family (the issue's climbers against stayers, as a regression): the rating change from the end of P1 to the end of P2 on the same improvement, Holm across ten;
  - confirmatory: the level family: the rating change from the end of P1 to the end of P3 on the P1 level, oriented so that a positive β means a better level, Holm across ten;
  - exploratory: the primary within each band (B0 to B3); whether the change predicts staying active in P3; the stage 2 cost from the pilot's variances.

## 5. The minimum effect that matters

- Minimum effect: 14 Elo of next-month rating change per standard deviation of improvement in the quantity.
- Why this size: 14 Elo against an equal opponent is about 0.02 points a game, ADR 0101's ranking floor (`RANK_MIN_EXCESS_PER_GAME`): the smallest gain the ranking already treats as worth a player's attention. A standard deviation of the month-to-month change is what separates a player who changed from one who did not in this cohort; a link smaller than 14 Elo for that much change would not justify moving a key up the teaching order.

## 6. Power analysis

- Method: a closed form for the registered test (`analysis.py power`): the HC3 t-test of β_q read at the simultaneous level 1 − 0.05/10, with the standard error σ_res / (√n · s_x), σ_res the residual SD of the outcome and s_x the SD of the standardised exposure net of the covariates, both from the pilot. Two powers per quantity: the chance of "precedes climbing" at β = 14, and of "below the minimum effect" at β = 0.
- Inputs: the pilot, every player whose hash fraction is in [0.9, 1), with the registered rules (development data, never read by the decision); its n per quantity scaled by the slice's width.
- Result: the confirmatory slice is [0, q), q the smallest of 0.1, 0.2, …, 0.9 at which every estimable quantity reaches 0.80 in both powers; if none does, q = 0.9 and this section states the power reached. Written in `power.json` (an input of the run) and here after the pilot, before the confirmatory run.
- Stage 2: the players the median key needs for 0.80 at the same level, times 80 games (40 in each feature month) at 39.1 CPU-seconds a game at 1 million nodes (EXP-0008). Stage 2 runs inside this experiment only at 20 CPU-hours or less; otherwise it is filed with its cost.
- Sensitivity: `power.json` gives every grid point for every quantity.

## 7. Design

- Unit of analysis: the player, one row each; games are nested in players and never counted as replicates. Every quantity is a sum over the player's games.
- The cohort (ADR 0114): `lichess-db cohort --salt 80b7ed61df63b5e94e96089d100887cef8465b13028260352639573bee9ae5d4 --player-rate 1` (the salt is `sha256("EXP-0009/cohort/salt")`, public by design) on the standard rated dumps of 2026-07 (P0), 2026-08 (P1, `--games`), 2026-09 (P2, `--games`) and 2026-10 (P3), from `https://database.lichess.org/standard/`. Each file's URL, size and verified SHA-256 are in its `meta.json` and in `inputs.sha256` (external inputs, ADR 0106 Amendment 1); the CC0 database, many thanks to Lichess and its players. Nothing per player is committed.
- Eligibility, in order, from the players files only: at least 20 rapid games in P0; at least 10 rapid games with `[%eval]` in each of P1 and P2; a P1 end rating below 2000. Then from the games: per player and feature month, the first 48 such games by `sha256(salt ‖ "game:" ‖ id)` are observed, games that are not standard chess from the start position or that observe with an error are dropped, and the first 40 left are kept; a player needs 10 in each feature month. The outcome needs at least 5 rapid games in P3; a player without them is in the attrition analysis only.
- Slices: the pilot is every player with a hash fraction in [0.9, 1); the confirmatory run is [0, q) with q from section 6. No player is in both.
- Seeds: none. Nothing is drawn at random: the selection is the hash order, and every interval is analytic. The results record the namespace `EXP-0009` for the arena.
- Development and confirmatory data: the pilot slice is development data; it sets q and the stage 2 cost, and its estimates decide nothing. Sections 2, 4, 5, 8 and 9 are committed before the pilot runs.
- Held fixed: `algo-4.0.0`; the start position's search at 1 million nodes, one thread, 16 MB of hash; Lichess's own evaluations for every other position, put where production puts a Lichess game's.
- Leakage controls: the exposures come from P1 and P2, the outcome from P3, the covariates from P0 to P2; nothing from P3 enters a feature or a selection rule except the outcome's own activity rule, which is reported as attrition. The design is player-grouped by construction (one row per player).
- Reuse: the October extract of EXP-0008 measured the observe rate and checked the parsing; none of its games is an input. Nothing else applies (section 0).

## 8. Analysis plan

- Model: for each quantity q, an ordinary least squares regression with HC3 standard errors (statsmodels), one row per player with q in both feature months:
  - primary: y = R_end(P3) − R_end(P2) on the standardised improvement Δq (P1 to P2, oriented so a positive value is better: a lower rate, a higher accuracy), with the P1 level of q, (R_end(P2) − 1500)/100, R_end(P2) − R_end(P1), R_end(P1) − R_end(P0), R_end(P0) − R_start(P0), log(1 + rapid games) in P1, P2 and P3, the rapid share of the player's games in P2, log(1 + opportunities) of q in P1 and P2, and the change in the mean (opponent − own) rating of the observed games from P1 to P2, divided by 100;
  - contemporaneous: y = R_end(P2) − R_end(P1) on the same Δq, with the P1 level, (R_end(P1) − 1500)/100, R_end(P1) − R_end(P0), R_end(P0) − R_start(P0), log(1 + rapid games) in P1 and P2, the P2 rapid share, the opportunities and the opponent gap change;
  - level: y = R_end(P3) − R_end(P1) on the standardised, oriented P1 level, with (R_end(P1) − 1500)/100, R_end(P1) − R_end(P0), R_end(P0) − R_start(P0), log(1 + rapid games) in P0 and P1, the P1 opportunities and the P1 opponent gap.
- Effect size: β_q in Elo per SD with its 80%, 95% and 97.5% intervals and the simultaneous interval at 1 − 0.05/10 (Bonferroni, 99.5%).
- Multiple comparisons: Holm at 0.05 across the ten quantities within each family; the families are reported separately and only the primary decides.
- Reads, per quantity: "precedes climbing" when Holm rejects and β > 0; "precedes falling" when Holm rejects and β < 0; "below the minimum effect" when the simultaneous interval lies inside (−14, 14); "undetermined" otherwise; "not estimable" with fewer than 30 players.
- Diagnostics: no fitted Bayesian model. A quantity with fewer than 30 players is not estimable.
- Exclusions and missing data: decided in section 7; a key-month below 5 opportunities is missing for that key only; complete cases per quantity. No outlier removal.

## 9. Decision rule

Measurement on one version, with three outcomes read on the primary family, in this order:

1. **Candidates to teach:** at least one of the eight keys reads "precedes climbing". We publish them as candidates, never as causes, and register a comparison experiment of new trainability rows on new months (the next one, with this result as its development data). No production change here.
2. **No data-driven order from these keys:** no key reads "precedes climbing", at least six of the eight read "below the minimum effect", and the positive control (accuracy) reads "precedes climbing", which shows the design can see a link of this kind. We keep the rows and say plainly that we have no data-driven teaching order yet.
3. **Need more data:** anything else. At most one follow-up, registered with `Follows:` this experiment, on new months (never pooled with these players), with its own power analysis. A second "need more data" keeps the rows.

Stage 2 (section 6): it runs here only at 20 CPU-hours or less; otherwise the decision names it with its cost.

This departs from ADR 0106's measurement rule (one interval in a band): there is no band a link should lie in; the question is whether each key's link is shown, shown small, or neither, and the positive control guards against reading an insensitive design as a null.

### Stopping rules

- Early stopping: none. The run costs about one CPU-hour once the dumps are streamed; a look would save nothing.
- Futility: none, for the same reason. The pilot's power analysis plays that part: it sets q, and a stage 2 past its ceiling is not run.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0009/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`; its `pilot` and `power` stages run before, in that order)
- Analysis code sha256: `f2595e362fcd30b9594975acb092524691e3b161a94d1359844118281b9e9486`

## 11. Deviations

None.
