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
- If the result is a ship (the worst coverage's distance from 0.80 shrinks, its 97.5% interval wholly on the better side, every guard held), we will make the challenger production's rule as a major algorithm version, regenerate the replay goldens and `bench/baseline.json`, and close #13 if the secondary below says it is fully fixed; if not, the ranges stay as they are, the site says from EXP-0002 where they run narrow, and the next experiment is drafted from what failed.
- Expected cost: 51,200 profile computations (two rules on 6,400 synthetic players at four checkpoints), about 10 minutes on 6 cores of the lab container at EXP-0002's measured 31 a second a core; the Tier 1 guards come from EXP-0005's side-by-side run (5,120 games), which that experiment pays for; no AI tokens; about two days of maintainer time.
- Value score: impact 3 × uncertainty 3 ÷ cost 1 = 9
- Cheaper alternative? EXP-0002's generator and design are reused unchanged (its frozen `analysis.py` is imported), and the Tier 1 guards reuse EXP-0005's run. A closed-form check of the challenger without the comparison would skip the guards on false weaknesses and on the ranking, which a lighter prior can move; the development runs already showed the false-weakness guard is close (`dev/README.md`), so it has to be measured.
- Compute budget: 51,200 profile computations; cap 150%

## 1. Question

When Rematch shows "you miss a mate in 1 to 4 in 63% of your chances (80% range 55–71%)", does that range hold the player's true rate four times in five? For a player whose rate is two to eight times their peers' in odds it does not: EXP-0002 measured 0.10 for W4.missed at 30 games, because a band prior worth 40 chances outweighs a player who has about one mate chance a game, and the recency weights discount the data but not the prior. The challenger (section 3) caps the prior at two of the player's own games, counts the weighted data at their effective sample size, and divides that by the dispersion of the per-game counts. This experiment asks whether that brings every one of EXP-0002's 20 coverages into [0.75, 0.85] without telling more players at their peers' rate that they have a weakness, and without costing the Tier 1 benchmark its detections.

## 2. Hypotheses

- H0: the challenger's worst coverage is no closer to 0.80 than the champion's (ΔD ≤ 0, section 4).
- H1: it is closer by at least 0.05 (ΔD ≥ 0.05), the development runs' expectation being far more (0.69 against 0.05, `dev/README.md`).

Rival explanations of a ship, and what separates them:

- The challenger only widens every range: a wider range covers more. Separated by the ρ = 1 guard (a range too wide covers more than 0.80 at the peers' rate, which the guard penalises) and by the exploratory share of misses on each side and the median width.
- The development seeds flatter the challenger: it was chosen as the best of fifteen rules on them, so its development D is optimistic. Separated by fresh confirmatory seeds (`EXP-0006`, disjoint from `EXP-0006-dev`).
- The generator's assumptions (peer rates, chances per game, a design effect of 1.5) favour a rule that estimates dispersion. This cannot be separated here; the decision names it, and the Tier 1 guards on engine-generated players are the check that does not share the generator.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Reference | `algo-1.1.1+…` (the build), `ALGO_VARIANT` unset: `PriorRule::CHAMPION` | – |
| Candidate | the same build, `ALGO_VARIANT=challenger`: `PriorRule::CHALLENGER` = `{prior_games: 2, effective_counts: true, dispersion: true}` | in `estimate_weakness`: the data's `(k, n)` become the weighted rate over `n_eff = (Σ w n)² / Σ w² n` divided by the Pearson dispersion of the per-game counts (clamped to [1, 10]); the band prior's strength becomes `min(n0[key], 2 × n / games_in_window)`. W5's flat prior keeps its strength. The timeline's own posteriors, the ranking rule and every other component are unchanged |

`algo-1.1.1` is `algo-1.1.0` plus the variant code, with no output change (the replay goldens are byte-identical, `algorithm/CHANGELOG.md`). The candidate was chosen on development seeds by the rule in `dev/README.md`, written before the first development run, and is frozen as `PriorRule::CHALLENGER`. If it ships, it becomes `algo-2.0.0`.

## 4. Primary metric and ground truth

- Primary metric (exactly one): ΔD = D(champion) − D(challenger), where D is the largest |coverage − 0.80| over EXP-0002's 20 key × checkpoint coverages (W1, W4.missed, W7.traps, W9b and W5 at 30, 60, 120 and 250 games; ρ in {2, 4, 8}; two games a day; each pooled over band and dispersion with cells weighted equally). Positive is better.
- Ground truth: the per-chance rate each synthetic player's events were drawn from (EXP-0002's generator).
- Guarded KPIs (non-inferiority, section 9):
  - the coverage at the peers' rate (ρ = 1, every key and checkpoint): |coverage − 0.80| may grow by at most 0.05;
  - false weaknesses: the share of credible estimates (`credible`, ADR 0101) among players at or below the peers' rate (ρ in {0.5, 1}, every key and checkpoint) may grow by at most 0.01;
  - Tier 1 recall at 2 at 80 games over EXP-0005's 45 flawed players (pilot and smoke), paired: the lower end of the 80% Newcombe interval of the difference at least −0.10 (EXP-0001's smoke tolerance);
  - clean false alarms: no more clean checkpoints with a rank-1 weakness than the champion over EXP-0005's three clean controls (14 checkpoints), as `bench::results::regressions` allows none;
  - ranking stability: the smoke suite's top-2 agreement after dropping 10% of the games may fall by at most 0.25 (`bench/baseline.json`'s tolerance).
- Secondary, confirmatory (it decides #13 only, never the ship): "fully fixed", every one of the challenger's 20 coverages with its Bonferroni interval for 20 wholly inside [0.75, 0.85], and #13's own acceptance on EXP-0005's smoke suite (the challenger's coverage in [0.65, 0.95] and `bench-hang` ranked W1 first at 80 games).
- Exploratory: each rule's 20 coverages with 95% intervals and the share missing on the high side; coverage and credible share by ρ; the #13 cell (W1, peers at 0.0008, true rate 0.023) by checkpoint and dispersion, Wilson 95%.

## 5. The minimum effect that matters

- Minimum effect: ΔD = 0.05.
- Why this size: 0.05 is the half-width of the target band. A change in the worst coverage smaller than that cannot move a cell from one side of the band to its middle, and would not change what the range's words ("80% range") mean to a player.

## 6. Power analysis

- Method: a normal approximation of the ship rule, with the standard errors of a share over the players a coverage pools. Each primary coverage pools 18 cells × n players (one interval each, independent within a coverage), so at a coverage near the champion's worst (0.11) and the challenger's (0.85) the standard errors are sqrt(0.11 × 0.89 / 18n) and sqrt(0.85 × 0.15 / 18n), and the paired ΔD's standard error is at most their root sum of squares (the two rules see the same players, which only lowers it). The bootstrap of a maximum is biased upwards for the rule whose worst cell is a near-tie; at these differences the bias is far smaller than the effect.
- Inputs: the coverages of the development runs (`dev/README.md`), two-sided 97.5% for the ship rule.
- Result: 200 players a cell (EXP-0002's size). The ship rule needs much less (the table); the size is set by the confirmatory secondary, which classifies each of the challenger's 20 coverages with a Bonferroni half-width of about 0.020 (EXP-0002 section 6: a true 0.80 lands inside the band with probability above 0.99, a true 0.82 or 0.78 about 0.93), and by the false-weakness guard, whose margin of 0.01 is narrow (the development difference was 0.0089). At 200 a cell the whole run is 51,200 profile computations, about 10 minutes.
- Sensitivity, the ship rule's power at ΔD = 0.05:

| Players a cell | SE of ΔD | Power |
|---|---|---|
| 25 | 0.022 | 0.50 |
| 50 | 0.016 | 0.82 |
| 100 | 0.011 | 0.99 |
| 200 | 0.008 | > 0.99 |

## 7. Design

- Unit of analysis: the simulated player. Every interval of a player (5 keys × 4 checkpoints) is resampled together, and the same resampled players are used for both rules.
- Pairing and blocking: paired (both rules on the same draws, each cell from its own named seed); blocked by band × ρ × dispersion (EXP-0002's 30 cells, equal players each).
- Seeds: EXP-0002's names in this experiment's namespace, `EXP-0006`: `players/<band>/<rho>/<disp>` for the 30 cells and `issue13/none`, `issue13/de1.5`; and `bootstrap/paired`, one numpy generator that draws the 2,000 resamples of player indices shared by every cell and both rules. No other randomness.
- Development and confirmatory data: the challenger was developed on `EXP-0006-dev` (100 players a cell, the two-games-a-day schedule); every seed here is in `EXP-0006`, so the draws are disjoint. The Tier 1 players (EXP-0005) were never used in development.
- Held fixed: EXP-0002's design (keys, chances, peer rates, bands, ρ grid, dispersions, checkpoints, the reference rows, the two-games-a-day schedule); the every-game-in-a-day schedule is not run (it separated the weighting in EXP-0002 and decides nothing here).
- Data: `inputs.sha256` lists EXP-0002's `analysis.py` (the generator), EXP-0001's `analysis.py` (`newcombe_paired`, `z_of`) and EXP-0005's `results.json` (the Tier 1 guards). EXP-0005's line is added when its results are committed, before this experiment runs; until then this experiment cannot run. No real player's games.
- Leakage controls: not a predictive question.
- Reuse: EXP-0002's generator and design; EXP-0005's side-by-side run of the same challenger for every Tier 1 guard (no engine time here); EXP-0001's interval code.

## 8. Analysis plan

- Model or test: no model. Each interval scores 1 when `low ≤ θ ≤ high`. A coverage is the mean over its cells of the mean over players. D per rule is the largest |coverage − 0.80| over the 20 primary coverages; ΔD = D(champion) − D(challenger).
- Effect size: ΔD with its 80%, 95% and 97.5% percentile intervals from a paired stratified bootstrap (2,000 resamples of players within each cell, the same resample for every cell and both rules, D recomputed in each). Each rule's 20 coverages with 95% percentile intervals and Bonferroni intervals for 20 (the estimate ± 3.02 bootstrap standard errors). The two profile guards as paired differences with 80% percentile intervals from the same resamples. Recall at 2 with the 80% Newcombe (1998, method 10) interval of the paired difference. Clean false alarms and top-2 agreement as counts and values (no interval: three clean players and one smoke suite).
- Multiple comparisons: one primary decides. The five guards are each a non-inferiority condition that must all hold, which only makes a ship harder, so no correction applies. The secondary's 20 classifications use Bonferroni intervals for 20. The exploratory results are uncorrected.
- Diagnostics: no Bayesian fit. The run stops if the `algorithm_id` changes between calls, a response is out of order, the two rules or EXP-0005 ran on different builds, or the budget is exceeded.
- Exclusions and missing data: none. Every estimate has an interval.

## 9. Decision rule

Comparison (two versions), with the template's default rule:

- Ship: the 97.5% interval of ΔD lies wholly above 0, and every guard holds: the upper end of the 80% interval of the ρ = 1 deviation's growth is at most 0.05; the upper end of the 80% interval of the false-credible share's growth is at most 0.01; the lower end of the 80% Newcombe interval of recall at 2 is at least −0.10; the challenger has no more clean rank-1 checkpoints than the champion; and the smoke top-2 agreement falls by at most 0.25.
  - Margins: ρ = 1 deviation 0.05; false-credible share 0.01; recall at 2 0.10; clean rank-1 checkpoints 0; top-2 agreement 0.25.
- Reject: the 97.5% interval of ΔD lies wholly below 0.05, or a guard lies wholly beyond its margin (the lower end of its 80% interval beyond it; for the counts, the count itself beyond it).
- Need more data: anything else. At most one follow-up, registered with `Follows: EXP-0006`, the challenger unchanged, its own power analysis, and the same rule on the pooled players. A second "need more data" is a reject.

On a ship: `PriorRule::CHALLENGER` becomes production's rule as `algo-2.0.0` (a major version: profile output changes), with the replay goldens regenerated, `bench/baseline.json` rebuilt from a smoke run of the new version, an entry in `algorithm/CHANGELOG.md` citing EXP-0006 and ADR 0109, and "Fixes #13" only when the secondary says fully fixed. On anything else: production is unchanged, and the decision drafts the next experiment from the condition that failed.

### Stopping rules

- Early stopping: none. The run is 51,200 profile computations, about 10 minutes; a look would save nothing.
- Futility: none, for the same reason.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0006/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`)
- Analysis code sha256: `50501688e3645b4c488ea9cf5c1e3fa0db013257a7d6ac1ac9ce0f8b0bf2d9f6`

## 11. Deviations

None.
