# EXP-0009: what actually helps people climb

- Status: pre-registered
- Issue: #104
- Author: the maintainer, with Claude (lane `exp/0009-what-helps`)
- Kind: measurement (one version, `algo-4.0.0`, on an observational cohort)
- Follows: none
- Registered: 2026-11-24. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: the teaching order. Today it is set by `trainability` (`backend/crates/profile-core/src/constants.rs`, rows per band from `docs/research/improvement-engine.md` section 6.4), which multiplies every weakness's priority (`stats/priority.rs`) and comes from research notes, not from players who improved. This experiment measures something narrower than trainability: whether a natural fall in a weakness comes before a rise in rating, which is evidence about the weakness's link to results, not about how well teaching moves it; and it pools the bands B0 to B3, so it cannot set a per-band row. With it, whether stage 2 of issue #104 (our own analysis at 1 million nodes, for the nine keys that need the enrichment) is worth its engine time.
- If the result is that at least one key's reduction precedes climbing (section 9, outcome 1), we will register a comparison experiment of a re-ordered teaching priority on new months, per band, with this result as its development data (then a major algorithm version, in its own lane); this stage changes no production row itself. If no key's reduction precedes climbing, at least six of the eight are shown below the minimum effect and the positive control holds, we will keep the rows and say plainly on the site that we have no data-driven teaching order yet; otherwise one follow-up with more months. Outcomes 2 and 3 change a statement on the site and the stage 2 decision, not the product. Stage 2 runs inside this experiment only if its powered size costs at most 20 CPU-hours at 1 million nodes; otherwise it is filed with its cost for the owner to decide.
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
- Noise in the rates: rates from 10 to 40 games are noisy, and the exposure is standardised by its observed SD, which is mostly noise for a rare key; a true effect per true SD shows up per observed SD multiplied by √reliability. Unhandled, that would push real links into "below the minimum effect". So the minimum effect is read per observed SD as 14 × √reliability, with each exposure's split-half reliability measured in the run (section 8), and the positive control must also hold before a null reads as "no order".

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-4.0.0+51733d23895f` | – |

From `cli algo show` on the cli built from this branch. The bench crate gains `bench::observe`, the cli `cli lab observe` and `lichess-db` its `cohort` command; none is in the manifest, and the id is unchanged (`node scripts/algorithm/check.mjs`).

## 4. Primary metric and ground truth

- Primary metric (exactly one family, Holm across its nine members): β_q for each of the nine quantities below (the eight keys and accuracy), each read on its own regression.
- The quantities, per player and feature month, from `cli lab observe` (the production `evaluate_game` on Lichess's evaluations, `build_move_records`, `game_accuracy`, `compute_game_features` with no enrichment and no opening explorer, `observe_game`):
  - the eight rankable keys whose detectors read only the evaluations, the clocks, the board and the result (`bench::observe::EVAL_ONLY_KEYS`): W4.missed, W4.allowed, W5, W6, W7.traps, W7.prep, W9a and W9b, each as events over opportunities summed over the month's games (missing below 5 opportunities);
  - the mean game accuracy, the positive control: averaged over every move, the most precise quantity, and the one rating catch-up should link to climbing. It shows the design can see a link, not that a noisy key's null is real, which is why each key's own read uses its reliability.
  - W7.prep is the production detector with the opening's end at ply 20 for every game (`DEFAULT_BOOK_EXIT_PLY`, production's value without an opening explorer); with the explorer, production ends the opening at the book exit, so W7.prep here is the self-hosted value, not the hosted one.
- Ground truth: the players' Lichess rapid ratings in the monthly dumps (CC0): the mean of the ratings after the last five rapid games of a month (its end) and before the first five (its start).
- Not measured, and why: W1, W2, W3.shallow, W3.deep, W8.basics, W8.advanced, W9c, W10 and W11 need the enrichment (MultiPV lines, the refutation and threat probes, the tablebase probe, Maia); without it W1 would count opportunities and never an event. They are stage 2. The ranked weaknesses need every key and the band reference; also stage 2. "Which fell first against the rating's change point" needs more than two feature months; not measured.
- Secondary metrics:
  - confirmatory: the contemporaneous family (the issue's climbers against stayers, as a regression): the rating change from the end of P1 to the end of P2 on the same improvement, Holm across nine; descriptive only for W5, W6 and accuracy, which are partly defined by the same games' results;
  - confirmatory: the level family: the rating change from the end of P1 to the end of P3 on the P1 level, oriented so that a positive β means a better level, Holm across nine;
  - sensitivity: the primary with the post-exposure terms (the rating change within P2 and the change in the opponent gap) added, and the primary with inverse-probability-of-staying weights;
  - exploratory: the blunder rate (blunders over the player's moves) on the primary's model; the primary within each band (B0 to B3); whether the change predicts staying active in P3; the stage 2 cost from the pilot.

## 5. The minimum effect that matters

- Minimum effect: 14 Elo of next-month rating change per standard deviation of the true improvement in the quantity; per observed standard deviation, the unit the coefficient is in, 14 × √reliability (section 8).
- Why this size: 14 Elo against an equal opponent is about 0.02 points a game, ADR 0101's ranking floor (`RANK_MIN_EXCESS_PER_GAME`): the smallest gain the ranking already treats as worth a player's attention. A standard deviation of the true month-to-month change is a unit every key shares, where a natural unit (a fall of ten points in the rate) means something different for W4.missed and W9a; a link smaller than 14 Elo for that much change would not justify moving a key up the teaching order. Disattenuating by the split-half reliability keeps noise from passing for a small effect.

## 6. Power analysis

- Method: a closed form for the registered read (`analysis.py power`): the HC3 z-test of β_q at the simultaneous level 1 − 0.05/9 (Bonferroni standing in for Holm, slightly conservative), with the pilot's HC3 standard error scaled by √(n_pilot / n). Two powers per quantity: the chance of "precedes climbing" at a true 14 Elo per true SD (14 × √reliability per observed SD), and of "below the minimum effect" at 0. The pilot writes only n, the standard errors and the reliabilities (`pilot/pilot.json`), never an estimate or a read.
- Inputs: the pilot, every player whose hash fraction is in [0.9, 1), with the registered rules (development data, never read by the decision); its n per quantity scaled by the slice's width.
- Result: the confirmatory slice is [0, q), q the smallest of 0.1, 0.2, …, 0.9 at which at least six of the eight keys reach 0.80 in both powers and accuracy reaches 0.80 for "precedes climbing" (what outcome 2 needs); if none does, q = 0.9 and this section states the power reached. Written in `power.json` (an input of the run) and here after the pilot, before the confirmatory run.
- The pilot (`pilot/pilot.json`, hash fraction in [0.9, 1)): 28,721 players with 20 or more rapid games in P0, 6,365 of them with 10 or more analysed games in P1 and P2, 5,820 below 2000, 5,818 with 10 observed games in both months, 5,472 active in P3; 305,043 games observed, 54 failed. The split-half reliability of the month-to-month change is low for every quantity: 0.01 (the floor; ρ ≤ 0) for W4.missed, 0.026 W7.prep, 0.044 W4.allowed, 0.047 W7.traps, 0.071 accuracy, 0.10 W9a, 0.12 W9b, 0.14 W5 and 0.15 W6. A change over one month of 10 to 40 analysed games is mostly noise.
- So q = 0.3 (`power.json`): six keys reach 0.80 in both powers (W4.allowed, W5, W6, W7.prep, W7.traps, W9a) and accuracy reaches 1.00; W9b 0.75 and 0.50, W4.missed 0.15 and 0.00. About 915,000 games to observe, inside the budget of 1.2 million.
- Stage 2 at this design would need about 5,240 players for the median key, 80 games each at 39.1 CPU-seconds: about 4,560 CPU-hours, far past its 20. It does not run here.
- Stage 2: the players the median key needs for 0.80 at the same level, times 80 games (40 in each feature month) at 39.1 CPU-seconds a game at 1 million nodes (EXP-0008). Stage 2 runs inside this experiment only at 20 CPU-hours or less; otherwise it is filed with its cost.
- Sensitivity: `power.json` gives every grid point for every quantity.

## 7. Design

- Unit of analysis: the player, one row each; games are nested in players and never counted as replicates. Every quantity is a sum over the player's games.
- The cohort (ADR 0114): `lichess-db cohort --salt 80b7ed61df63b5e94e96089d100887cef8465b13028260352639573bee9ae5d4 --player-rate 1` (the salt is `sha256("EXP-0009/cohort/salt")`, public by design) on the standard rated dumps of 2026-07 (P0), 2026-08 (P1, `--games`), 2026-09 (P2, `--games`) and 2026-10 (P3), from `https://database.lichess.org/standard/`. Each file's URL, size and verified SHA-256 are in its `meta.json` and in `inputs.sha256` (external inputs, ADR 0106 Amendment 1); the CC0 database, many thanks to Lichess and its players. Nothing per player is committed.
- Eligibility, in order, from the players files only (the count each step leaves is reported, and the final cohort by band): at least 20 rapid games in P0; at least 10 rapid games with `[%eval]` in each of P1 and P2; a P1 end rating below 2000. Then from the games: per player and feature month, the first 48 such games by `sha256(salt ‖ "game:" ‖ id)` are observed, games that are not standard chess from the start position or that observe with an error are dropped, and the first 40 left are kept; a player needs 10 in each feature month. The outcome needs at least 5 rapid games in P3; a player without them is in the attrition analysis only.
- Slices: the pilot is every player with a hash fraction in [0.9, 1); the confirmatory run is [0, q) with q from section 6. No player is in both.
- Seeds: none. Nothing is drawn at random: the selection is the hash order, and every interval is analytic. The results record the namespace `EXP-0009` for the arena.
- Development and confirmatory data: the pilot slice is development data; it sets q and the stage 2 cost, and its estimates decide nothing. Sections 2, 4, 5, 8 and 9 are committed before the pilot runs.
- Held fixed: `algo-4.0.0`; the start position's search at 1 million nodes, one thread, 16 MB of hash; Lichess's own evaluations for every other position, put where production puts a Lichess game's.
- Leakage controls: the exposures come from P1 and P2, the outcome from P3, the covariates from P0 to P2; nothing from P3 enters a feature, a covariate or a selection rule except the outcome's own activity rule, which is handled as attrition (section 8). The design is player-grouped by construction (one row per player). Players of the two slices can meet in a game, and a rating change is zero-sum within it, so the run counts the games with an opponent in the other slice.
- Reuse: the October extract of EXP-0008 measured the observe rate and checked the parsing; none of its games is an input. Nothing else applies (section 0).

## 8. Analysis plan

- Model: for each quantity q, an ordinary least squares regression with HC3 standard errors and normal (z) intervals (statsmodels), one row per player with q in both feature months. Every change model adjusts for terms measured before the outcome window and not caused by the exposure's rating effect: the P1 level of q (oriented), (R_end(P1) − 1500)/100, R_end(P1) − R_end(P0), R_end(P0) − R_start(P0), log(1 + rapid games) in P0, P1 and P2, the rapid share of the player's games in P1, the analysed share (rapid games with `[%eval]` over rapid games) in P1 and in P2, and log(1 + opportunities) of q in P1 and P2.
  - primary: y = R_end(P3) − R_end(P2) on the standardised improvement Δq (P1 to P2, oriented so a positive value is better: a lower rate, a higher accuracy) and those terms. The rating change within P2 and the change in the opponent gap are left out: they are partly consequences of the improvement (a player who got better starts to climb within P2 and meets stronger opponents), so adjusting for them would remove part of what is asked;
  - adjusted (sensitivity): the primary plus R_end(P2) − R_end(P1) and the change in the mean (opponent − own) rating of the observed games from P1 to P2, divided by 100;
  - contemporaneous: y = R_end(P2) − R_end(P1) on the same Δq, with the same terms and the opponent gap change;
  - level: y = R_end(P3) − R_end(P1) on the standardised, oriented P1 level, with (R_end(P1) − 1500)/100, R_end(P1) − R_end(P0), R_end(P0) − R_start(P0), log(1 + rapid games) in P0 and P1, the P1 rapid share, the P1 analysed share, the P1 opportunities and the P1 opponent gap / 100.
- Reliability: for each quantity and model, the split-half reliability of the exposure on the players the model used: q computed on the month's odd and on its even games by game key, the change (or level) on each half, their correlation ρ, and Spearman-Brown 2ρ / (1 + ρ), floored at 0.01 (a ρ at or below 0 gives 0.01). The minimum effect per observed SD is 14 × √reliability, and β / √reliability is reported as the disattenuated effect.
- Effect size: β_q in Elo per observed SD with its 80%, 95% and 97.5% intervals and the simultaneous interval at 1 − 0.05/9 (Bonferroni, 99.44%), and the disattenuated β.
- Multiple comparisons: Holm at 0.05 across the nine quantities within each family; the families are reported separately and only the primary decides.
- Reads, per quantity: "precedes climbing" when Holm rejects and β > 0; "precedes falling" when Holm rejects and β < 0; "below the minimum effect" when the simultaneous interval lies inside (−14 × √reliability, 14 × √reliability); "undetermined" otherwise (a quantity without a reliability cannot read "below the minimum effect"); "not estimable" with fewer than 30 players.
- Attrition: staying active in P3 (5 rapid games or more) on the primary's exposure and terms, a linear probability model with HC3; and the primary re-fitted by weighted least squares with weights 1 / p̂, p̂ from a logistic model of staying on the same terms (floored at 0.05). Both are sensitivities and decide nothing.
- Diagnostics: no fitted Bayesian model. A quantity with fewer than 30 players is not estimable; a logistic fit that fails is reported as failed.
- Exclusions and missing data: decided in section 7; a key-month below 5 opportunities is missing for that key only; complete cases per quantity. No outlier removal.

## 9. Decision rule

Measurement on one version, with three outcomes read on the primary family (the reads of section 8, "below the minimum effect" against 14 × √reliability), in this order:

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
- Analysis code sha256: `b7c6f6e50da02955c5066d0b8ecdcd83f5cb3aca8bb4a3acf2333bf90c8856e8`

## 11. Deviations

- 2026-11-26, after the results, recorded by the lane (Claude) from the critique of the result (`review.md`, part two). None changes the decision.
  - Section 0 says a stage 2 past its ceiling is "filed with its cost for the owner to decide". It is named with its cost in `decision.md` (about 4,560 CPU-hours) and not filed as an issue: the price is for a design whose positive control failed, and EXP-0029 prices it again.
  - The primary's outcome baseline, R_end(P2), is built partly from the same games as the exposure, so its drift back from September's luck biases every β towards negative. Sections 2 and 8 did not name this; it is the likeliest reason the control failed, and EXP-0029 must fix it.
  - The reads use the point estimate of each reliability, whose standard error is near 0.008; section 8 did not register its uncertainty.
  - The four dumps were streamed with July and October twice, by a mistake in the lane's script; the files are the second, verified copies, identical in content.
