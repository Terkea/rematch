# Experiments

One folder per experiment, numbered in order of registration. Nothing here is deleted: a rejected or inconclusive experiment stays, with its decision.

```
lab/experiments/EXP-NNNN/
  preregistration.md   from docs/experiments/TEMPLATE.md, committed before anything runs
  analysis.py          the single entry point: python analysis.py --out DIR
  inputs.sha256        "<sha256>  <path from this folder>" for every input it reads
  results.json         the numbers, the algorithm_id they ran on and the compute budget
                       against the actual (canonical JSON)
  figures/             the figures, made from results.json's data
  review.md            the critic's review of the pre-registration and of the result
  decision.md          ship, reject or need more data, and a "## Next" of roadmap issues
  summary.md           the site's summary; once decided, live, rerun and the cost of one run
  arena.py             once decided: the comparison rule the arena uses (ADR 0109)
```

## The order of commits

CI checks the first three against git (`python -m rematch_lab.experiments check --history`).

1. `preregistration.md`, `analysis.py` and `inputs.sha256`, with the sha256 of `analysis.py` in section 10 (`python -m rematch_lab.experiments hash EXP-NNNN`). The pull request is reviewed before the experiment runs.
2. `review.md`, part one: a fresh critic reads the pre-registration with the `scientific-critical-thinking` and `peer-review` skills, looking for p-hacking room, leakage, pseudoreplication, an effect size picked to pass, and a decision rule that cannot fail. Fixes are made in the pre-registration, in their own commit before any result, and `review.md` lists each edit.
3. `results.json` and `figures/`, from one run of the frozen `analysis.py`, in a commit after the registration. From this commit on, sections 0 to 10 of the pre-registration and `analysis.py` are frozen.
4. `review.md`, part two: the same review of the result (overclaiming, deviations, diagnostics).
5. `decision.md`, the line in `lab/findings.md`, and, for a shipped change, the entry in `algorithm/CHANGELOG.md` with the EXP id, and an ADR.

## What `analysis.py` may do

- Read only the inputs in `inputs.sha256`.
- Call the algorithm only through `rematch_lab`; never re-implement a detector, an estimate or the ranking.
- Draw randomness only from `rematch_lab.experiments.seed(exp_id, name)`, with every name listed in section 7.
- Count what it spends with `rematch_lab.experiments.Budget`, built from section 0's compute budget: it stops the run at the cap (150% unless section 0 registers more), and `budget.record()` goes into `results.json` as `compute`. Counted units only (games, nodes, profile calls, tokens): wall time goes in `decision.md`, since `results.json` must be byte-identical.
- Write only `results.json` (with `rematch_lab.experiments.write_results`, including the `algorithm_id` of the responses it used) and `figures/` under `--out`.

CI (`.github/workflows/lab.yml`) reruns it twice and fails when the two `results.json` differ, or when they differ from the committed one while its `algorithm_id` is the current cli's. Once the algorithm has moved on, a decided experiment stays live: the arena re-runs it against the champion on its schedule (ADR 0109), and its other rules are still checked.

## `review.md` and `decision.md`

`review.md` lists each concern with where it is, what was seen, why it matters and what was done about it, in two parts (before the run, after it). `decision.md` states the decision against the pre-registered rule, the primary effect with its intervals, the deviations, and the follow-up experiment (at most one, with `Follows:`) or the algorithm version.

## Closing one unrun

The kill rule (ADR 0107): when a pre-registered experiment no longer informs an open decision, it is closed with a one-line reason in its `summary.md` (`status: decided`, the reason as the first bullet) and in `lab/findings.md`, and it is not run.

## Once decided: live

A decided experiment stays live (ADR 0109). Its `summary.md` front matter adds `live: true`, `rerun: per-pr|weekly|release` and the measured cost of one run (`run_cpu_hours`, `run_minutes`, from `decision.md`), and `arena.py` holds its comparison rule: `METRICS` (each metric's `name`, `role` primary or guard, `label` and `better`), `measure(results)` (the compact per-unit data a comparison needs), `metrics(measures)` (each metric's value and interval) and `compare(champion, challenger)` (each metric's effect, interval, level and verdict, by the experiment's registered rule). An experiment that no longer informs a decision says `live: false` and `retired: <reason>`. Its `decision.md` ends with a `## Next` whose items each name a roadmap issue, at least one an experiment.
