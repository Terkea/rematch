# EXP-NNNN: <the question, in one line>

<!--
The pre-registration of one experiment on the algorithm (ADR 0106). Copy this file to
lab/experiments/EXP-NNNN/preregistration.md, fill every section, and commit it with
analysis.py BEFORE anything is run. The lab CI checks the order in git: both files must be
added in a commit strictly before the one that adds results.json. Once results.json is
committed, sections 1 to 10 are frozen (CI compares them with their text in that commit) and
analysis.py must keep the sha256 of section 10. A change after that is a deviation in section 11
(text only, never a new analysis.py) or a new experiment. Edits between registration and the run,
such as the fixes a critic asks for, stay visible in git and are listed in review.md.

The skills in .claude/skills/science/ say how to fill each section: hypothesis-generation (2),
statistical-power (6), experimental-design (7), statistical-analysis, statsmodels and pymc (8).
Delete these comments when you fill the file.
-->

- Status: pre-registered | running | decided (ship, reject or need more data)
- Issue: #<number>
- Author:
- Kind: comparison (two algorithm versions) | measurement (one version)
- Follows: EXP-NNNN, whose decision was "need more data" | none
- Registered: <date>. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

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

<!-- From `cli algo show` (ADR 0104). A measurement study lists the one version it measures. The
candidate exists as a branch before registration and does not change afterwards: not after the
results, and not between this experiment and a follow-up. -->

## 4. Primary metric and ground truth

- Primary metric (exactly one):
- Ground truth: <injected rates and the generator's tallies (Tier 1), the Lichess reference
  sample's held-out games (Tier 2), Stockfish, Syzygy, the Lichess puzzle themes>
- Secondary metrics: <each one marked confirmatory or exploratory>

<!-- Definitions as in docs/benchmark.md and the KPI catalogue of the evaluation spec. Only the
primary metric decides. Every metric named here is reported whatever it shows; a secondary or
exploratory finding can motivate a new experiment, never a ship. -->

## 5. The minimum effect that matters

- Minimum effect:
- Why this size:

<!-- The smallest difference that would change something for a player, in the metric's own
units: "0.02 points per game is one game's result every 50 games, about 14 Elo (ADR 0101)";
"0.1 of recall at 2 is one flawed player in ten told the wrong focus". Never a size picked so a
measured value clears a threshold, and never Cohen's conventions. -->

## 6. Power analysis

- Method: <simulation of the decision rule in section 9 (statistical-power's
  `.claude/skills/science/statistical-power/scripts/simulate_power.py`), or a closed form
  (`scripts/power.py` there) when the rule is exactly that test>
- Inputs: <the baseline rate and its source, the between-player variance or ICC and its source,
  the cluster sizes, and the sidedness: two-sided>
- Result: <players per arm, games per player, seeds>
- Sensitivity: <the n needed across the plausible range of the inputs; a table or a curve>

<!-- Power is the probability that the section 9 rule says "ship" (comparison) or "within the
band" (measurement) when the true effect is the minimum effect; the target is at least 80%. It is
a-priori only: observed power after the run is not reported (it restates the p-value). If the
feasible n cannot reach 80%, change the design, or register a measurement study by the interval
width it will reach. -->

## 7. Design

- Unit of analysis: <the player; games and positions are nested in players, so they are never
  counted as independent replicates>
- Pairing and blocking: <the same seeds for both versions; blocks by rating band and flaw type>
- Seeds: <every name passed to `rematch_lab.experiments.seed(exp_id, name)`, listed here; no
  other source of randomness>
- Development and confirmatory data: <the seeds or players the candidate was developed and
  tuned on, and the confirmatory ones, which are disjoint from them>
- Held fixed: <Stockfish build and nodes, the suite, the opponents, the clock model>
- Data: <every input with its sha256 in `inputs.sha256` (checked by CI): a RunRecord from
  bench-history, a fixture, a pinned Lichess extract. No real chess.com player's games.>
- Leakage controls: <for predictive questions: chronological split per player, player-grouped
  folds, nothing after the split date in the features>

## 8. Analysis plan

- Model or test: <exactly what analysis.py fits, with its formula and family>
- Effect size: <the estimand, with its 80%, 95% and 97.5% intervals (Wilson for shares, a
  seeded bootstrap over players for means, the posterior for Bayesian models)>
- Multiple comparisons: <the family (the secondary metrics, the rating bands) and the
  correction (Holm), or why none is needed>
- Diagnostics: <for PyMC: 4 chains, the draws and the seeds; R-hat at most 1.01, bulk and tail
  ESS at least 400 in total across the 4 chains, no divergences. A run that fails a diagnostic
  is a failed result, reported as such, never a number.>
- Exclusions and missing data: <what is dropped, by which rule, decided now>

## 9. Decision rule

<!-- Written before the results, in terms of the intervals above. Keep the default rule for your
kind of study or replace it, and say why. -->

Comparison (two versions). The decision interval is two-sided 97.5%, not 95%, because the rule
allows one follow-up and the two looks split the 5% error between them (Bonferroni):

- Ship: the 97.5% interval of the primary effect lies wholly on the better side of zero, and no
  guarded KPI (ADR 0104) crosses its non-inferiority margin: the worse end of its 80% interval
  of the difference stays inside the margin registered here.
  - Margins: <one per guarded KPI, e.g. recall at 2: 0.05; top-1 precision: 0.05>
- Reject: the 97.5% interval lies wholly below the minimum effect (the change cannot be as
  large as what matters), or a guarded KPI's 80% interval lies wholly beyond its margin.
- Need more data: anything else. At most one follow-up, registered with `Follows:` this
  experiment, the candidate unchanged, its own power analysis, and the same 97.5% rule on the
  pooled players. A second "need more data" is a reject.

Measurement (one version):

- Within the band: the 95% interval of the primary metric lies wholly inside the target band
  registered here, e.g. coverage of the 80% ranges in [0.75, 0.85].
- Outside the band: the 95% interval lies wholly outside it. The follow-up is a comparison
  experiment for a fix.
- Need more data: anything else, under the same one-follow-up rule.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-NNNN/analysis.py` (`python analysis.py --out DIR` writes
  `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`)
- Analysis code sha256: `<64 hex characters: python -m rematch_lab.experiments hash EXP-NNNN>`

<!-- Only this line in section 10 counts. The lab CI refuses an analysis.py whose sha256 differs
from it once results.json is committed, and reruns the analysis twice to check results.json byte
for byte while the experiment's algorithm_id is the current one. -->

## 11. Deviations

<!-- Filled after registration only. Each: the date, what changed from sections 1 to 10, why,
who decided, and what it can change in the conclusion. A deviation never replaces analysis.py
after results exist; that is a new experiment. "None." when there are none. -->
