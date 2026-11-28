# EXP-0005: how much do the rankings move between two real algorithm versions?

- Status: decided (closed unrun by the value gate, review concern 1)
- Issue: #100
- Author: the maintainer, with Claude (lane `exp/0005-0006`)
- Kind: measurement (one pair of versions): the churn EXP-0001 assumed
- Follows: none (EXP-0001's "Next" 1; EXP-0001 was decided "budget-bound", not "need more data")
- Registered: 2026-11-24. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: the size of the full and smoke suites and the smoke suite's CI false-alarm rate, which EXP-0001 set under an assumed churn of 0.10 (`backend/crates/bench/src/suite.rs`, `bench/baseline.json`, EXP-0001 section 9).
- If the result is a churn whose 95% interval lies wholly below 0.10, we will re-run EXP-0001's section 9 rule at the interval's upper end and resize the full suite to what it picks (the cheapest suite reaching 80% power for +0.1 within 9,000 games, or the budget's suite with its new minimum detectable change); if it lies wholly above 0.10, we will re-register the weekly suite's detectable change at the interval's lower end, larger than 0.15; if it straddles 0.10, the suites stay as they are.
- Expected cost: one side-by-side run of EXP-0001's 32 pilot players (3,840 games) and the smoke suite (1,280 games) with **Stockfish 19** at 12,000 nodes and 8 engines, about 35 minutes and 3.5 CPU-hours at EXP-0001's measured 0.39 seconds a game; the challenger's profiles are built on the same games, so they cost no engine time; under two minutes for the re-sizing simulation; no AI tokens; about half a day of maintainer time.
- Value score: impact 2 × uncertainty 3 ÷ cost 2 = 3
- Cheaper alternative? The reference side is already stored (EXP-0001's `results.json`, per player and checkpoint), but a churn needs both versions on the same games, and EXP-0001 kept no top-2 lists, so the reference is rebuilt beside the challenger in the same run, at no extra engine cost; the rebuilt side doubles as the between-runs check against the stored one. The same run is EXP-0006's Tier 1 guard, which EXP-0006 needs whatever this experiment shows, so the marginal cost of the churn is the analysis alone. No literature gives the churn of this pipeline's ranking.
- Compute budget: 5,120 games (3,840 pilot, 1,280 smoke) and 172,000 simulated suites (the largest branch of section 9: 2 game counts × 20 suite sizes × 4,000, the 10-step detectable-change curve × 4,000 and 3 false-alarm cells × 4,000); cap 150%

## 1. Question

How often does a real candidate lose a flawed player's correct top-2 weakness that the reference found, and how often does it find one the reference missed? EXP-0001 sized the benchmark under an assumed churn of 0.10, and the churn moved the players needed for +0.1 in recall at 2 from 14 to 40 a flaw at 80 games (`lab/experiments/EXP-0001/decision.md`). A chess example: `bench-hang` hangs a piece on 5% of its eligible moves and the reference ranks W1 first at 80 games; if the challenger's lighter prior lets a noisy W9b overtake W1 and push it to third, that is one lost hit, and the suite needs more players to see a real gain through such losses.

The first real candidate is EXP-0006's challenger (`PriorRule::CHALLENGER` in `backend/crates/profile-core/src/stats/variant.rs`, frozen before this registration). One candidate's churn estimates the churn of one kind of change, a change to the prior of the 80% ranges; a detector or ranking change can churn more or less, and the decision says so.

## 2. Hypotheses

A measurement, so the hypotheses name the branches of section 9.

- H0: the churn at 80 games is at least 0.10 (EXP-0001's assumption is not conservative).
- H1: the churn at 80 games is below 0.10.

The expectation written before the run: the challenger changes the prior of every key, so priorities and credibility move, but a flaw the reference finds sits well above its benchmark, so it stays in the top 2; we expect a churn near 0 and a few gains (W4.missed and W7.traps, whose ranges EXP-0002 found pulled down hardest). Rival explanations of a non-zero churn, and how each is separated:

- The pipeline is not reproducible between builds or runs: separated by the between-runs check (the rebuilt reference side against EXP-0001's stored rows, field by field) and by the byte-identical double run; any mismatch is reported and makes the churn uninterpretable.
- The second account sees different games: excluded by construction (the same generated games are inserted for both accounts) and by `backend/crates/bench/tests/variant.rs` (`a_side_by_side_variant_sees_the_same_games`).
- A hit lost to a new false weakness rather than to a demoted target: the lost players' top-2 lists are reported.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Reference | the build's id (`algo-1.1.1+…`), `ALGO_VARIANT` unset: `PriorRule::CHAMPION` | – |
| Candidate | the same build, `ALGO_VARIANT=challenger`: `PriorRule::CHALLENGER` | the estimate's prior rule only (`profile_core::stats::priority::estimate_weakness`); see EXP-0006 section 3 |

The pipeline up to the profile is shared: the variant changes nothing before `estimate_weakness`, so both sides see the same facts. `algo-1.1.1` differs from EXP-0001's `algo-1.1.0` by the variant code only, with no output change (the replay goldens are byte-identical), which the between-runs check confirms on the pilot.

## 4. Primary metric and ground truth

- Primary metric (exactly one): the churn at 80 games, the share of the reference's top-2 hits the candidate loses, `n01 / (n11 + n01)`, over every flawed player of the pilot (30) and the smoke suite (15), one checkpoint per player, with a Jeffreys 95% interval.
- Ground truth: each player's injected flaw (its target key); a hit is the target in the CoachingInput's top 2 (`docs/benchmark.md`, score a).
- Secondary metrics:
  - confirmatory: the between-runs check (the number of mismatched fields between the rebuilt reference side and EXP-0001's stored pilot rows; expected 0) and the determinism of both sides;
  - confirmatory: the gain rate `n10 / (n10 + n00)` at 80 games with its Jeffreys 95% interval;
  - exploratory: the churn by suite and by checkpoint (30, 45, 60, 80 and 120 games), the churn pooled over checkpoints with the player as the unit (a cluster bootstrap), the share of players (clean controls included) whose top-2 set, top-2 order or rank 1 changes, the clean controls' rank-1 weaknesses, the target intervals' coverage on both sides, both scorecards, and the smoke CI rule's false-alarm rate at the measured churn.

## 5. The minimum effect that matters

- Minimum effect: a churn interval that excludes 0.10, in either direction.
- Why this size: 0.10 is the value EXP-0001's decision rests on. Below it the weekly suite is larger than it needs to be (or can detect a smaller change); above it the suite detects less than it registered. A churn of 0.05 against 0.10 already halves the players needed (EXP-0001: 28 against 40 a flaw at 80 games).

## 6. Power analysis

- Method: exact, from the binomial: for a true churn b and n reference hits, the probability of each classification of the Jeffreys 95% interval (`analysis.py --design`, `design_power`).
- Inputs: the reference hits at 80 games, 19 in EXP-0001's pilot and 9 in the smoke suite (`bench/baseline.json`, recall at 2 of 0.6 over 15), so n = 28; two-sided.
- Result, n = 28 (computed by `analysis.py --design` on this branch before the run):

| True churn | Below 0.10 | Straddles | Above 0.10 |
|---|---|---|---|
| 0 | 1.00 | 0 | 0 |
| 0.02 | 0.57 | 0.43 | 0.00 |
| 0.05 | 0.24 | 0.76 | 0.00 |
| 0.10 | 0.05 | 0.93 | 0.02 |
| 0.20 | 0.00 | 0.68 | 0.32 |
| 0.30 | 0.00 | 0.22 | 0.78 |

- Sensitivity: with the pilot alone (n = 19) the interval can never lie below 0.10 (no losses give a Jeffreys 95% upper end of 0.124), which is why the smoke suite is added; n = 40 would need the full suite's 8,960 games for a small gain (0.45 against 0.57 at b = 0.02, because one loss of 40 straddles). The design can say "below" only for a churn near 0 and "above" only for one near 0.3; between, it straddles, and the suites stay. That is a weak design for an intermediate churn, written down here so the decision cannot read more into a straddle than it holds. The run is paid for by EXP-0006's guards, so this experiment's marginal cost is the analysis.
- At b = 0.10 the rule says "below" 5% of the time (all 28 hits kept), the Jeffreys interval's error at its lower tail for this n.

## 7. Design

- Unit of analysis: the player. One checkpoint per player decides (80 games); the pooled-checkpoint secondary resamples players, never checkpoints.
- Pairing and blocking: paired by construction (both versions on the same games in the same run); blocked by flaw (equal players per flaw in each suite).
- Seeds: the players are EXP-0001's registered pilot (EXP-0001's `pilot_suite()`, its own seeds `player/<flaw>/<j>` and `player/clean/<j>` in EXP-0001's namespace, imported unchanged; `analysis.py` stops if the suite differs from the one stored in EXP-0001's `results.json`) and the named smoke suite (seeds 11 to 16, 21 to 25 and 31 to 35). This experiment's own seeds: `power/resize`, `power/mde`, `power/false_alarm`, `bootstrap/churn`, each the root of a numpy generator. No other randomness.
- Development and confirmatory data: the candidate was developed on EXP-0006's development seeds (`EXP-0006-dev`), profile-only synthetic players that share nothing with these engine-generated players.
- Held fixed: **Stockfish 19** at 12,000 nodes, 16 opponents, the clock model, the policy and injectors of `bench`, 8 engines (the count changes the speed, never the facts).
- Data: `inputs.sha256` lists EXP-0001's `analysis.py` (its `pilot_suite`, `hits_at`, `simulate_recall` and `required_m` are reused, not copied) and `results.json` (the stored reference rows). No real player's games.
- Leakage controls: not a predictive question.
- Reuse: EXP-0001's pilot players and its stored per-player reference results (the between-runs check, and the hit probabilities of the re-sizing simulation, which stay EXP-0001's so that only the churn changes); EXP-0001's simulation code; the run itself is reused by EXP-0006 as its Tier 1 guard. No cached engine evaluations: the bench runs in a throwaway database with its in-memory cache (ADR 0100), and a rerun costs the same games.

## 8. Analysis plan

- Model or test: no model. The paired 2 × 2 table of top-2 hits at 80 games (n11 both, n10 candidate only, n01 reference only, n00 neither) over the 45 flawed players; churn `n01 / (n11 + n01)` and gain `n10 / (n10 + n00)`.
- Effect size: the churn with its Jeffreys 80% and 95% intervals (Brown, Cai and DasGupta 2001; the ends are 0 or 1 when no or every hit is lost); the gain likewise; the pooled-checkpoint churn with a 95% cluster bootstrap (2,000 resamples of flawed players, seed `bootstrap/churn`).
- The re-sizing (section 9), with EXP-0001's frozen functions and its stored pilot rows: `simulate_recall` over EXP-0001's grid (20 suite sizes, 4,000 simulated suites each, G in {80, 120}, the +0.1 change), `required_m`, and EXP-0001's choice rule (the cheapest fitting suite in games, the larger G on a tie; otherwise the budget's suite). The detectable change of the budget's suite (22 a flaw at 80 games) over EXP-0001's change grid. The smoke CI rule's false-alarm rate (3 a flaw, the tolerance 0.1, true change 0) at the measured churn, the interval's upper end and 0.10.
- Multiple comparisons: one primary interval decides; the secondaries are estimates, not tests.
- Diagnostics: no Bayesian fit. The run stops if the pilot suite differs from EXP-0001's, the smoke suite is not 1,280 games, or the compute budget is exceeded.
- Exclusions and missing data: none. A flawed player with no checkpoint at 80 games would be left out of the primary and named; every registered player has one.

## 9. Decision rule

Measurement, on the Jeffreys 95% interval of the churn at 80 games:

- Below: the interval lies wholly below 0.10. EXP-0001's section 9 rule is rerun at the interval's upper end (the conservative end, as EXP-0001 took 0.10), and its choice becomes the full suite in `backend/crates/bench/src/suite.rs`: the cheapest (players a flaw, games) reaching 80% power for +0.1 within 9,000 games, or, if none fits, the budget's suite with its detectable change registered. The smoke suite keeps its size; its false-alarm rate at the upper end is reported.
- Above: the interval lies wholly above 0.10. The weekly suite keeps its size, and its detectable change, rerun at the interval's lower end, is registered in `docs/benchmark.md` and EXP-0001's decision is annotated.
- Straddles: anything else. The suites stay; the detectable change at the point estimate is reported, not adopted.

A mismatch in the between-runs check overrides the above: the churn is not interpreted, the suites stay, and the mismatch is filed as a bug. In every branch, the result is the churn of one change to the prior, written as such in `docs/benchmark.md`; the next real candidate of another kind (a detector or the ranking) is measured the same way before the suites move again. "Need more data" does not apply: a straddle keeps the suites, which is the action for an uninformative result, and the next candidate's run adds players.

### Stopping rules

- Early stopping: none. The run is one pass of 5,120 games, and the interval is read once.
- Futility: none; the design's limits are stated in section 6.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0005/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`; `--design` prints section 6's table).
- Analysis code sha256: `a0d6f356ed92de68edcf51ef2ca705ff9f0304521320db9e42c7e38864c3b50d`

## 11. Deviations

1. 2026-11-24, after the critic's review (`review.md`, concern 1): closed unrun. The design cannot move the suites in any branch, so its outcomes lead to one action and ADR 0107 says it is not run. The churn is reported descriptively by EXP-0006, which owns the side-by-side run. Sections 0 to 10 are left as registered.

From the maths audit (2026-11-28, ADR 0118), recorded after the decision; sections 0 to 10 and `analysis.py` are unchanged and the closure stands (`math.md`, Findings):

1. Section 5's "halves" is a 30% cut (28 against 40).
2. On EXP-0001's grid a resize needs a churn upper end of about 0.022, which takes about 112 hits with none lost, not the 0.03 and 83 that `review.md` and `decision.md` give. The full and smoke suites at today's recall give about 79 hits, an upper end of 0.031, so EXP-0024 as planned in the decision's Next could not resize the suites even with no hit lost.
3. Rounding and attribution: 0.1223, not 0.124; the review's 0.51 and 0.82 are its own values (EXP-0001 published 0.525 and 0.826).
4. "Above" at x = 7 sits on the 0.15/0.20 edge.
5. Section 0's items sum to 212,000 simulated suites, not 172,000.
