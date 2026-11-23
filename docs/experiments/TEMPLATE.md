# EXP-NNNN: <the question, in one line>

<!--
The pre-registration of one experiment on the algorithm (ADR 0106). Copy this file to
lab/experiments/EXP-NNNN/preregistration.md, fill every section, and commit it, with
analysis.py, BEFORE anything is run. The commit that adds it is the registration; nothing in
sections 1 to 10 changes after the first result exists. A change after that is written up in
section 11 as a deviation, with its date and reason, or becomes a new experiment.

The skills in .claude/skills/science/ say how to fill each section: hypothesis-generation (2),
statistical-power (6), experimental-design (7), statistical-analysis, statsmodels and pymc (8).
Delete these comments when you fill the file.
-->

- Status: pre-registered | running | decided (ship, reject or need more data)
- Issue: #<number>
- Author:
- Registered: <date>, commit `<sha of the commit that added this file>`

## 1. Question

<!-- What we want to know and why it matters to a player. One concrete chess example: "a 1500
player who hangs a piece in 2% of eligible moves is told so within 30 games". Link the gap or
issue that raised it (docs/benchmark.md, #13). -->

## 2. Hypotheses

- H0:
- H1:

<!-- State H1 as a direction and a size, not "it is better". List the rival explanations that
would give the same result (search noise between node-limited searches, the clock model, the
peer benchmark's games-against-you bias, a ceiling at 80 games) and the result that separates
each from H1. -->

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Reference | `algo-…` | – |
| Candidate | `algo-…` | <the manifest components and constants that change, e.g. `profile.source_digest` for `n0[W1]` 300 → 120> |

<!-- From `cli algo show` (ADR 0104). An experiment that changes no behaviour (EXP-0001 sizes
the benchmark) lists the one version it measures. The candidate exists as a branch before the
experiment runs; its code is not tuned after the results are seen. -->

## 4. Primary metric and ground truth

- Primary metric (one):
- Ground truth: <injected rates and the generator's tallies (Tier 1), the Lichess reference
  sample's held-out games (Tier 2), Stockfish, Syzygy, the Lichess puzzle themes>
- Secondary metrics: <each one marked confirmatory or exploratory>

<!-- Definitions as in docs/benchmark.md and the KPI catalogue of the evaluation spec. A metric
named here is reported whatever it shows. -->

## 5. The minimum effect that matters

- Minimum effect:
- Why this size:

<!-- The smallest difference that would change a decision, in the metric's own units, with the
reason: "0.1 of recall at 2 is one flawed player in ten of the full suite"; "0.02 points per game
is one game's result every 50 games, about 14 Elo (ADR 0101)". Not Cohen's conventions. -->

## 6. Power analysis

- Method: <closed form (scripts/power.py) or simulation (scripts/simulate_power.py), and why>
- Inputs: <alpha, target power, the baseline rate and its source, the between-player variance or
  ICC and its source, the cluster sizes>
- Result: <players per arm, games per player, seeds>
- Sensitivity: <the n needed across the plausible range of the inputs; a table or a curve>

<!-- A-priori only. Observed power after the run is not reported (it restates the p-value). If
the feasible n cannot reach 80% power for the minimum effect, say so here and either change the
design or register the experiment as a precision study with the interval width it will reach. -->

## 7. Design

- Unit of analysis: <the player; games and positions are nested in players, so they are never
  counted as independent replicates>
- Pairing and blocking: <the same seeds for both versions; blocks by rating band and flaw type>
- Randomisation and seeds: <how players, seeds and splits are drawn, and the seed of each draw
  (rematch_lab.experiments.seed)>
- Held fixed: <Stockfish build and nodes, the suite, the opponents, the clock model>
- Data: <every input with its checksum: a RunRecord from bench-history, a fixture, a pinned
  Lichess extract. No real chess.com player's games.>
- Leakage controls: <for predictive questions: chronological split per player, player-grouped
  folds, nothing after the split date in the features>

## 8. Analysis plan

- Model or test: <exactly what analysis.py fits, with its formula and family>
- Effect size: <the estimand, with its 80% and 95% intervals (Wilson for shares, a seeded
  bootstrap over players for means, the posterior for Bayesian models)>
- Multiple comparisons: <the family (the secondary metrics, the rating bands) and the
  correction (Holm), or why none is needed>
- Diagnostics: <for PyMC: 4 chains, the draws and the seeds; R-hat at most 1.01, bulk and tail
  ESS at least 400, no divergences. A run that fails a diagnostic is a failed result, reported
  as such, never a number.>
- Exclusions and missing data: <what is dropped, by which rule, decided now>

## 9. Decision rule

<!-- Written before the results, in terms of the intervals above. The default rule: -->

- Ship: the 95% interval of the primary effect lies wholly on the better side, the point
  estimate is at least the minimum effect, and no guarded KPI regresses clearly (its 80%
  interval of the difference lies wholly on the worse side, ADR 0104).
- Reject: the 95% interval lies wholly below the minimum effect, or on the worse side.
- Need more data: anything else. The follow-up is a new experiment with a new power analysis.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-NNNN/analysis.py` (`python analysis.py --out DIR` writes
  `DIR/results.json` and `DIR/figures/`)
- Analysis code sha256: `<64 hex characters: python -m rematch_lab.experiments hash EXP-NNNN>`

<!-- The lab CI refuses an analysis.py whose sha256 differs from this line once results.json is
committed, and reruns the analysis twice to check results.json byte for byte. -->

## 11. Deviations

<!-- Filled after registration only. Each: the date, what changed from sections 1 to 10, why,
who decided, and what it can change in the conclusion. "None." when there are none. -->
