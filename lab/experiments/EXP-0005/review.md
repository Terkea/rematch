# Review of the pre-registration (part one)

Reviewer: a fresh agent (Claude) with no part in writing the registration, using the vendored `scientific-critical-thinking` and `peer-review` skills, on 2026-11-24, against commit a57973a9. It read `docs/evaluation.md`, the template, ADRs 0106, 0107 and 0109, `lab/experiments/README.md`, this folder, EXP-0001's registration, decision, `analysis.py` and `results.json`, and the variant and harness code. Nothing was run in Docker; its calculations were plain host Python (Jeffreys and Newcombe intervals, and EXP-0001's `simulate_recall` re-implemented at 2,000 draws a cell, which reproduces EXP-0001's published powers of 0.51, 0.71 and 0.82 at 22, 32 and 40 a flaw).

What held up: the pairing by construction (the reference account is analysed first and the second account's searches are cache hits; `bench/tests/variant.rs` shows the champion as the variant reproduces the main run), the between-runs check against EXP-0001's stored rows, and the honest statement in section 6 that the design is weak.

## Concerns and what was done

1. No branch of section 9 can change the suites, so the value case fails ADR 0107's gate (blocker).
   - Seen: with 28 reference hits, "below" needs no hit lost; the Jeffreys 95% upper end is then 0.0851. At that churn EXP-0001's simulation gives power for +0.1 at 80 games of 0.56 at 22 a flaw, 0.76 at 32 and 0.85 at 40, so the rule needs 40 a flaw (16,160 games), above the 9,000-game budget: "budget-bound", the suite `suite.rs` already has, with its detectable change still 0.15 on the grid. "Above" needs at least 7 of 28 lost and changes one line of `docs/benchmark.md`; "straddles" keeps the suites. The expected result (a churn near 0, section 2) leads to today's configuration. A resize would need an upper end near 0.03, about 83 reference hits with none lost.
   - Done: closed unrun under the kill rule (option a of the review). EXP-0006 owns the side-by-side run and reports the churn by checkpoint and by flaw as a descriptive result, never acted on. `decision.md` records it; section 11 of the registration says so; sections 0 to 10 are unchanged. Enlarging the design (option c) was not taken: with concern 2's fix, a prior change could not move the suites down whatever its churn, so the action would still be the same.
2. One candidate, of a kind expected not to churn, stands in for "the churn between versions" (major).
   - Done: recorded in `decision.md` as the rule for the next attempt: the suites move down only on the churn of a candidate that changes a detector or the ranking, or on the largest churn over the candidates measured; a prior change's churn is a lower bound. EXP-0006 names its churn "the churn of the EXP-0006 prior change".
3. The budget-bound branch does not compute what section 9 says (major).
   - Done: moot, as the experiment does not run. The re-registration will carry the fix (the detectable change at the chosen G, EXP-0001's choice of G).
4. Nothing checks that the challenger was applied or that both sides saw the same facts (major).
   - Done: in EXP-0006's `analysis.py`, which now owns the run: it stops unless every player's variant is `challenger`, every checkpoint's unweighted events and chances agree between the sides, at least one estimate's interval differs (a positive control), both sides are deterministic, and the reference side equals EXP-0001's stored pilot field by field.
5. A third of the hits sit outside the between-runs check (minor). Done in EXP-0006: the smoke reference scorecard is compared with `bench/baseline.json` in its decision.
6. Losses may cluster by flaw (minor). Done in EXP-0006: the per-flaw table at 80 games is reported.
7. The "confirmatory" gain decides nothing (minor). Done in EXP-0006: descriptive.
