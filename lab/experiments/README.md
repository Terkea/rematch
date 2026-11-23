# Experiments

One folder per experiment, numbered in order of registration. Nothing here is deleted: a rejected or inconclusive experiment stays, with its decision.

```
lab/experiments/EXP-NNNN/
  preregistration.md   from docs/experiments/TEMPLATE.md, committed before anything runs
  analysis.py          the single entry point: python analysis.py --out DIR
  results.json         the numbers, written by analysis.py (canonical JSON)
  figures/             the figures, made from results.json's data
  review.md            the critic's review of the pre-registration and of the result
  decision.md          ship, reject or need more data, and what happens next
```

## The order of commits

1. `preregistration.md` and `analysis.py`, with the sha256 of `analysis.py` in section 10 (`python -m rematch_lab.experiments hash EXP-NNNN`). The pull request is reviewed before the experiment runs.
2. `review.md`, part one: a fresh critic reads the pre-registration with the `scientific-critical-thinking` and `peer-review` skills, looking for p-hacking room, leakage, pseudoreplication, an effect size picked to pass, and a decision rule that cannot fail. Problems are fixed before the run, in the pre-registration.
3. `results.json` and `figures/`, from one run of the frozen `analysis.py`.
4. `review.md`, part two: the same review of the result (overclaiming, deviations, diagnostics).
5. `decision.md`, the line in `lab/findings.md`, and, for a shipped change, the entry in `algorithm/CHANGELOG.md` with the EXP id, and an ADR.

## What `analysis.py` may do

- Read only checksummed inputs (a RunRecord, a fixture, a pinned extract), and check the checksums.
- Call the algorithm only through `rematch_lab`; never re-implement a detector, an estimate or the ranking.
- Draw randomness only from `rematch_lab.experiments.seed(exp_id, name)`.
- Write only `results.json` (with `rematch_lab.experiments.write_results`) and `figures/` under `--out`.

CI (`.github/workflows/lab.yml`) reruns it twice and fails when the two `results.json` differ, when they differ from the committed one, or when `analysis.py` moved after its results were committed.

## `review.md` and `decision.md`

`review.md` lists each concern with where it is, what was seen, why it matters and what was done about it, in two parts (before the run, after it). `decision.md` states the decision against the pre-registered rule, the primary effect with its 80% and 95% intervals, the deviations, and the follow-up issue or algorithm version.
