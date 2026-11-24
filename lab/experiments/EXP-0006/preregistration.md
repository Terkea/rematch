# EXP-0006: does a lighter, honestly counted prior bring the 80% ranges into [0.75, 0.85]?

- Status: pre-registered
- Issue: #101 (and #13)
- Author: the maintainer, with Claude (lane `exp/0005-0006`)
- Kind: comparison (two algorithm versions)
- Follows: none (EXP-0002's "Next" 1; EXP-0002 was decided "outside the band", not "need more data")
- Registered: 2026-11-24. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: whether the challenger's prior rule replaces production's behind every 80% range (`backend/crates/profile-core/src/stats/priority.rs`, `estimate_weakness`; ADR-R2-5), which EXP-0002 found miscalibrated for players well above their peers (#13).
- If the result is a ship (the challenger's worst coverage within 0.07 of 0.80 at the 97.5% level, better than the champion's, and every guard held), we will make the challenger production's rule as a major algorithm version, regenerate the replay goldens and `bench/baseline.json`, and close #13 if the secondary below says #13's case is fixed; if not, the ranges stay as they are, the site says from EXP-0002 where they run narrow, and the next experiment is drafted from the condition that failed.
- Expected cost: 118,400 profile computations (two rules on 19,600 synthetic players at four checkpoints), about 25 minutes on 6 cores of the lab container at EXP-0002's measured 31 a second a core; one side-by-side Tier 1 run of 6,720 games (EXP-0001's 32 pilot players, the 16 smoke players and 20 new clean controls) with **Stockfish 19** at 12,000 nodes and 8 engines, about 45 minutes and 4.5 CPU-hours at EXP-0001's measured 0.39 seconds a game, the challenger's profiles built on the same games at no engine cost; no AI tokens; about two days of maintainer time.
- Value score: impact 3 × uncertainty 3 ÷ cost 2 = 4.5
- Cheaper alternative? EXP-0002's generator and design are reused unchanged (its frozen `analysis.py` is imported), and the Tier 1 run reuses EXP-0001's pilot players and checks its reference side against EXP-0001's stored results. A closed-form check of the challenger without the comparison would skip the guards on false weaknesses and on the ranking, which a lighter prior can move; the development runs showed the false-weakness guard is close (`dev/README.md`), so it has to be measured. The same run measures, descriptively, the churn EXP-0005 was registered for (closed unrun, `../EXP-0005/decision.md`).
- Compute budget: 118,400 profile computations and 6,720 games; cap 150%

## 1. Question

When Rematch shows "you miss a mate in 1 to 4 in 63% of your chances (80% range 55–71%)", does that range hold the player's true rate four times in five? For a player whose rate is two to eight times their peers' in odds it does not: EXP-0002 measured 0.10 for W4.missed at 30 games, because a band prior worth 40 chances outweighs a player who has about one mate chance a game, and the recency weights discount the data but not the prior. The challenger (section 3) caps the prior at two of the player's own games, counts the weighted data at their effective sample size, and divides that by the dispersion of the per-game counts. This experiment asks whether that brings every one of EXP-0002's 20 coverages within reach of [0.75, 0.85] without telling more players at their peers' rate that they have a weakness, and without costing the Tier 1 benchmark its detections.

## 2. Hypotheses

- H0: the challenger's worst coverage is more than 0.07 from 0.80 (D(challenger) > 0.07, section 4).
- H1: it is within 0.07 (on the development seeds it was 0.05, W5 at 30 games; the champion's was 0.69).

Rival explanations of a ship, and what separates them:

- The challenger only widens every range: a wider range covers more. Separated by the ρ = 1 guard (a range too wide covers more than 0.80 at the peers' rate), by the ρ = 1 coverage per key and at ρ = 0.5 (reported), and by the share of misses on each side and the median width per cell (reported). The dispersion factor is clamped at 1 from below, so it widens on average even without overdispersion, most at 30 games and for keys with one chance a game; those reports are where that would show.
- The development seeds flatter the challenger: it was the best of fifteen rules on them, so its development D is optimistic. Separated by fresh confirmatory seeds (`EXP-0006`, disjoint from `EXP-0006-dev` and from the power pilot's `EXP-0006-power`).
- The generator's assumptions (peer rates, chances per game, a design effect of 1.5) favour a rule that estimates dispersion. Not separable here; the decision names it, and the Tier 1 guards on engine-generated players are the check that does not share the generator. Only the two-games-a-day schedule runs: under every-game-in-a-day the effective counts change nothing, and EXP-0002 already measured that schedule's contrast.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Reference | `algo-1.1.1+42d749f7c50b`, `ALGO_VARIANT` unset: `PriorRule::CHAMPION` | – |
| Candidate | the same build, `ALGO_VARIANT=challenger`: `PriorRule::CHALLENGER` = `{prior_games: 2, effective_counts: true, dispersion: true}` | in `estimate_weakness`: the data's `(k, n)` become the weighted rate over `n_eff = (Σ w n)² / Σ w² n` divided by the Pearson dispersion of the per-game counts (clamped to [1, 10]); the band prior's strength becomes `min(n0[key], 2 × n / games_in_window)`. W5's flat prior keeps its strength. The timeline's own posteriors, the ranking rule and every other component are unchanged |

`algo-1.1.1` is `algo-1.1.0` plus the variant code, with no output change (the replay goldens are byte-identical, `algorithm/CHANGELOG.md`). The two rules share one build, so `algorithm_id` cannot tell them apart; `analysis.py` checks that the challenger was applied (section 8). The candidate was chosen on development seeds by the rule in `dev/README.md`, committed with the margins before the first development result (commits badb094a and 784bbf85), and is frozen as `PriorRule::CHALLENGER`. If it ships, it becomes `algo-2.0.0`.

## 4. Primary metric and ground truth

- Primary metric (exactly one): D(challenger), the largest |coverage − 0.80| over EXP-0002's 20 key × checkpoint coverages (W1, W4.missed, W7.traps, W9b and W5 at 30, 60, 120 and 250 games; ρ in {2, 4, 8}; two games a day; each pooled over band and dispersion with cells weighted equally). Smaller is better.
- Ground truth: the per-chance rate each synthetic player's events were drawn from (EXP-0002's generator).
- Comparison condition: ΔD = D(champion) − D(challenger) must be above 0 at the 97.5% level (the challenger better than production). It is expected to hold by far (0.64 on the development seeds); it is a condition of the ship, not its question.
- Guarded KPIs (non-inferiority, section 9):
  - the coverage at the peers' rate (ρ = 1, every key and checkpoint): |coverage − 0.80| may grow by at most 0.05;
  - false weaknesses: the share of credible estimates (`credible`, ADR 0101) among players at or below the peers' rate (ρ in {0.5, 1}, every key and checkpoint) may grow by at most 0.01;
  - Tier 1 recall at 2 at 80 games over the 45 flawed players of EXP-0001's pilot and the smoke suite, paired: the lower end of the 80% Newcombe interval of the difference at least −0.10;
  - clean false alarms: the share of clean controls (EXP-0001's two, the smoke suite's one and 20 new ones, 23 players) with a rank-1 weakness at 80 games, paired: the upper end of the 80% Newcombe interval of the difference at most 0.10;
  - ranking stability: the smoke suite's top-2 agreement after dropping 10% of the games may fall by at most 0.25 (`bench/baseline.json`'s tolerance).
- Secondary, confirmatory (it decides #13 only, never the ship): #13 is fixed when the challenger's four W1 coverages are within the band (Bonferroni intervals for 20 wholly inside [0.75, 0.85]), the #13 cell (W1, peers at 0.0008, true rate 0.023, 1,600 players pooled over the dispersions) is within the band at each checkpoint (Wilson intervals with a Bonferroni z for 4), and #13's own acceptance holds on the smoke suite (the challenger's coverage in [0.65, 0.95] and `bench-hang` ranked W1 first at 80 games).
- Reported whatever they show: each rule's 20 coverages with 95% and Bonferroni intervals, the share missing on the high side and the median width over the true rate; whether all 20 are within the band; the ρ = 1 coverage and the false-credible share per key; the ρ = 0.5 coverage; the smoke reference scorecard against `bench/baseline.json`.
- Exploratory: the churn of this prior change (EXP-0005's measure): by checkpoint, the reference's top-2 hits the challenger loses and the misses it gains with Jeffreys 95% intervals, the table by flaw at 80 games, and the players whose top-2 set, order or rank 1 changes. It is the churn of one prior change, a lower bound for detector or ranking changes, and decides nothing.

## 5. The minimum effect that matters, and the margins

- Tolerance of the primary: D(challenger) ≤ 0.07. The target band is [0.75, 0.85] (EXP-0002: a range that holds the truth three times in four or seventeen times in twenty is still read correctly); 0.02 beyond it is EXP-0002's registered resolution of one coverage (its Bonferroni half-width at this size). A ship needs the worst of the 20 coverages within [0.73, 0.87] at the 97.5% level.
- Margins, in a player's terms:
  - ρ = 1 deviation, 0.05: a player at the peers' rate may see their ranges hold the truth up to 0.05 further from 0.80 than now, the band's own half-width.
  - False-credible share, 0.01: at most one more credible estimate in 100 for players at or below the peers' rate. A player with the five keys measured here sees at most about one more false weakness in 20 reports; a credible estimate still has to clear ADR 0101's excess floor before it ranks. This is the price of calibration a lighter prior pays: an honest 80% range lies wholly above the truth one time in ten, against the champion's, which is pulled to the peers. The margin was written in `dev/README.md` before the development results.
  - Recall at 2, 0.10: a tripwire for gross losses, not a test of non-inferiority at 0.10. With 45 players and 28 reference hits it holds with up to 2 net losses and fails at 3 losses with no gains; 0.10 is EXP-0001's smoke tolerance.
  - Clean false alarms, 0.10: one clean player in ten newly shown a top weakness, the size EXP-0001 treats as one in ten told the wrong thing.
  - Top-2 agreement, 0.25: `bench/baseline.json`'s tolerance, not derived; reported as the CI gate would read it.

## 6. Power analysis

- Method: a power pilot of the whole section 9 rule (`power.py`, its output `dev/power.json`). The registered profile design runs once on its own namespace, `EXP-0006-power`, at the registered sizes; its 2,000 paired bootstrap replicates stand for the sampling distribution of every profile statistic the rule reads, and in each replicate the rule is applied with the half-widths those sizes give. The share of replicates that ship is the profile rule's power at the pilot's values. The Tier 1 guards are simulated from the binomial at no true change (45 flawed players with EXP-0001's hit rate of 0.63 at 80 games, 23 clean players with an alarm rate of 0.2, each side discordant with probability 0.05 each way), since a pilot of them costs a full engine run.
- Inputs: the development runs' values (`dev/README.md`): D 0.69 against 0.05 (W5 at 30 games), a false-credible difference of 0.0089 against a margin of 0.01, a ρ = 1 deviation 0.07 smaller.
- Result: 200 players a cell at ρ in {2, 4, 8}, 800 at ρ in {0.5, 1} and 800 for each half of the #13 cell. The guard cells are four times the primary's because the false-weakness guard's margin is narrow against the development difference (0.0089 against 0.01), so its 80% interval must be about 0.001 wide; at 200 a cell its power was about one half (the critic's estimate). The #13 cell's 1,600 players give a Bonferroni half-width of about 0.025, so a true 0.80 can land within the band. The power pilot's numbers are in `dev/power.json` and quoted in `review.md`.
- Sensitivity: the primary alone needs about 50 players a cell at a true D of 0.05 against the tolerance 0.07 (a standard error of about 0.008 at 200, 0.016 at 50); the false-weakness guard sets the size of the ρ ≤ 1 cells; the Tier 1 guards' no-change power is in `dev/power.json`.

## 7. Design

- Unit of analysis: the simulated player in the profile part, the engine-generated player in the Tier 1 part. Every interval of a player (5 keys × 4 checkpoints) is resampled together, and the same resampled players are used for both rules; a Tier 1 player gives one pair of outcomes at 80 games.
- Pairing and blocking: paired (both rules on the same draws and the same games); blocked by band × ρ × dispersion (EXP-0002's 30 cells) and by flaw.
- Seeds: EXP-0002's names in this experiment's namespace, `EXP-0006`: `players/<band>/<rho>/<disp>` for the 30 cells and `issue13/none`, `issue13/de1.5`; `clean/<j>` for j in 0 to 19 (the new clean controls' u32 seeds, checked distinct from the pilot's and the named suites'); `bootstrap/paired`, one numpy generator that draws the resamples of player indices, cell by cell in plan order, shared by both rules. EXP-0001's pilot keeps its own registered seeds (its `pilot_suite()`, imported), and the smoke suite its named ones. No other randomness.
- Development and confirmatory data: the challenger was developed on `EXP-0006-dev` (100 players a cell); the power pilot used `EXP-0006-power`; every seed here is in `EXP-0006`, so the three are disjoint. No Tier 1 player was used in development.
- Held fixed: EXP-0002's design (keys, chances, peer rates, bands, ρ grid, dispersions, checkpoints, the reference rows, the two-games-a-day schedule); **Stockfish 19** at 12,000 nodes, 16 opponents, the clock model and the bench's policy and injectors; 8 engines (the count changes the speed, never the facts).
- Data: `inputs.sha256` lists EXP-0002's `analysis.py` (the generator), EXP-0001's `analysis.py` (`pilot_suite`, `newcombe_paired`, `z_of`, the named suites' seeds) and EXP-0001's `results.json` (the stored reference rows of the pilot). No real player's games.
- Leakage controls: not a predictive question. The registration is frozen before any confirmatory number exists: `analysis.py` holds the sha256 of sections 0 to 9 as they stand after the review's fixes, and stops if they change.
- Reuse: EXP-0002's generator and design; EXP-0001's pilot players, their stored reference results (the between-runs check) and its interval code; the smoke suite. No cached engine evaluations: the bench runs in a throwaway database with its in-memory cache (ADR 0100), and the challenger's side is served from that cache.

## 8. Analysis plan

- Model or test: no model. Each interval scores 1 when `low ≤ θ ≤ high`. A coverage is the mean over its cells of the mean over players. D per rule is the largest |coverage − 0.80| over the 20 primary coverages.
- Effect size: D(challenger) with its 80%, 95% and 97.5% percentile intervals from a paired stratified bootstrap (2,000 resamples of players within each cell, the same resample for every cell and both rules, D recomputed in each); ΔD likewise. Each rule's 20 coverages with 95% percentile intervals and Bonferroni intervals for 20 (the estimate ± 3.02 bootstrap standard errors). The two profile guards as paired differences with 80% percentile intervals from the same resamples. Recall at 2 and the clean alarm with the 80% Newcombe (1998, method 10) interval of the paired difference. Top-2 agreement as values. The #13 cell with Wilson intervals at a Bonferroni z for 4. The churn with Jeffreys 95% intervals.
- Integrity of the Tier 1 run, each a stop: every player's side-by-side variant is `challenger`; at every checkpoint both sides have the same unweighted events and chances for every key (the same facts); at least one estimate's interval differs between the sides (a positive control that the rule was applied); both sides are deterministic; the reference side of the pilot equals EXP-0001's stored rows field by field (rank 1, target rank, hit, estimate, coverage); the pilot suite equals EXP-0001's; the smoke suite is 1,280 games; the Tier 1 run and the profiles ran on one build.
- Multiple comparisons: one primary decides, with the comparison condition and the five guards all required, which only makes a ship harder, so no correction applies. The secondary's classifications use Bonferroni intervals (for 20 and for 4). The rest is reported, uncorrected.
- Diagnostics: no Bayesian fit. The run also stops if the `algorithm_id` changes between calls, a response is out of order, sections 0 to 9 changed, or the budget is exceeded.
- Exclusions and missing data: none. Every estimate has an interval; every registered Tier 1 player has a checkpoint at 80 games.

## 9. Decision rule

Comparison (two versions), with the template's 97.5% level for the primary and 80% for the guards:

- Ship: the 97.5% interval of D(challenger) lies wholly at or below 0.07, the 97.5% interval of ΔD lies wholly above 0, and every guard holds: the upper end of the 80% interval of the ρ = 1 deviation's growth is at most 0.05; the upper end of the 80% interval of the false-credible share's growth is at most 0.01; the lower end of the 80% Newcombe interval of recall at 2 is at least −0.10; the upper end of the 80% Newcombe interval of the clean-alarm difference is at most 0.10; and the smoke top-2 agreement falls by at most 0.25.
  - Margins: ρ = 1 deviation 0.05; false-credible share 0.01; recall at 2 0.10; clean alarm 0.10; top-2 agreement 0.25 (section 5).
- Reject: the 97.5% interval of D(challenger) lies wholly above 0.07, or a guard lies wholly beyond its margin (the other end of its 80% interval beyond it; for top-2 agreement, the value itself).
- Need more data: anything else. At most one follow-up, registered with `Follows: EXP-0006`, the challenger unchanged, its own power analysis and new data: fresh profile seeds (`EXP-0006-followup`) for the profile guards, and the full suite's players side by side for the Tier 1 guards, with its cost in its section 0; the same rule on the pooled players. A second "need more data" is a reject.

On a ship: `PriorRule::CHALLENGER` becomes production's rule as `algo-2.0.0` (a major version: profile output changes), with the replay goldens regenerated, `bench/baseline.json` rebuilt from a smoke run of the new version, an entry in `algorithm/CHANGELOG.md` citing EXP-0006 and ADR 0110, and "Fixes #13" only when the secondary says #13 is fixed. On anything else: production is unchanged, and the decision drafts the next experiment from the condition that failed.

### Stopping rules

- Early stopping: none. The profile part is 118,400 computations (about 25 minutes) and the Tier 1 part one pass; a look would save little and spend error.
- Futility: none, for the same reason.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0006/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`)
- Analysis code sha256: `0000000000000000000000000000000000000000000000000000000000000000`

## 11. Deviations

None.
