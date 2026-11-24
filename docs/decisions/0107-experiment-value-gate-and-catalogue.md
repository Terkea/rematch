# 0107. The experiment value gate, and the catalogue that checks the learning framework

Date: 2026-11-24. Status: accepted (lane `expcat`, branch `docs/experiment-catalogue`). Builds on ADRs 0104 and 0106.

## Context

ADR 0106 made every behaviour change wait for a pre-registered experiment, and the first four (EXP-0001 to EXP-0004) answer questions we already had. Most of what Rematch tells a player has no experiment behind it: the motif tags, the 300,000-node search, the teaching order (`trainability`, from the research notes), the weekly target (ADR 0091), the trajectory verdict (ADR 0089), the trainer's schedule (ADR 0090). The owner asked "Don't we need more experiments? To factually check our learning framework", and then "Make sure that every experiment we run is actually valuable and not just wasting resources and time."

An experiment costs CPU-hours, sometimes AI tokens, and always maintainer time. One whose two outcomes lead to the same action buys nothing, and one that runs past the point where its answer is clear burns compute for no decision.

## Decision

1. **A value case comes first.** Section 0 of every pre-registration (`docs/experiments/TEMPLATE.md`) and the first block of every experiment issue states: the decision it informs; what we will do if the result is X and what if it is Y (if both lead to the same action, the experiment is not run); the expected cost (CPU-hours, wall time, AI tokens in USD, maintainer time); the value score, impact (1 to 3) × uncertainty (1 to 3) ÷ cost (1 to 3); and the cheaper alternative (a smaller design, existing data, a cached run, a simulation, the literature), which is used instead when it answers the question. `scripts/github/settings-lib.mjs` refuses an experiment issue without it, or with wrong arithmetic; the lab's check refuses a pre-registration without it, except EXP-0001 and EXP-0002, which were registered before this gate.
2. **The smallest sufficient design.** The sample size comes from the power analysis, never "as big as possible".
3. **Pre-registered stopping rules.** Where decisive, a group-sequential or Bayesian stopping boundary is registered with its looks, and so is a futility boundary for a run that can no longer reach a decision. Each look spends error, so the boundaries come from the registered rule's simulation.
4. **Reuse is stated.** Cached engine evaluations (`position_evals`), shared synthetic players, cohorts and prior runs are reused across experiments, and section 7 names what is reused, with its checksum in `inputs.sha256`.
5. **A compute budget with a hard cap.** Section 0 registers a budget in counted units (games, engine nodes, profile calls, AI tokens), and `results.json` records the budget against the actual (`compute`). The run aborts at 150% of the budget unless section 0 registers a higher cap (`rematch_lab.experiments.Budget`). Wall time and CPU seconds go in `decision.md`, because `results.json` must be byte-identical from run to run.
6. **The kill rule.** A pre-registered experiment that no longer informs an open decision is closed with a one-line reason, not run.
7. **The catalogue.** Twelve experiments, EXP-0007 to EXP-0018, in five epics by layer, after pruning the eighteen proposed: facts (v0.3), recommendation (v0.3), profile (v0.3), language (v0.3), learning (v0.6). EXP-0005 and EXP-0006 are the follow-ups of EXP-0001 and EXP-0002, drafted with their decisions. The run order is the facts first (EXP-0007, EXP-0008), then EXP-0009 ("what actually helps people climb", the top priority), then the rest by value score; the profile experiments need no engine and run alongside. `docs/evaluation.md` has the table.

## Options considered

1. **Run every proposed experiment.** Rejected: six of the eighteen ended in the same action whatever they showed, or were answered more cheaply elsewhere (the pruned list in `docs/evaluation.md`).
2. **A value score with weights or a finer scale.** Rejected for now: three levels each are coarse enough to agree on in a review, and the score orders the queue, it does not decide anything alone.
3. **The compute cap in wall time.** Rejected: wall time differs by machine and cannot go in a byte-identical `results.json`; counted units can.

## Consequences

- Every experiment issue in `.github/settings/roadmap-issues.json`, EXP-0001 to EXP-0004 included, opens with its value case.
- An experiment's `analysis.py` creates a `Budget`, counts what it spends, and writes `compute` into `results.json`.
- EXP-0009 needs a cohort selection keyed on the player hash alone, across months; that selection gets its own ADR under ADR-R2-11's privacy rules before any data is built.
- EXP-0011, EXP-0012, EXP-0017 and EXP-0018 measure rules written in TypeScript, which the lab's bridge does not call yet; each registration says how the lab calls them.
