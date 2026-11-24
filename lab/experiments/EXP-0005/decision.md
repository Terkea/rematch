# EXP-0005: decision

- Decision: closed unrun by the value gate (ADR 0107, the kill rule). The critic showed (`review.md`, concern 1) that no branch of section 9 changes the suites: with 28 reference hits the most favourable result, no hit lost, gives a Jeffreys 95% upper end of 0.0851, at which EXP-0001's rule still finds no suite within 9,000 games and keeps the one `backend/crates/bench/src/suite.rs` has. Its outcomes lead to one action, so it is not run.
- Algorithm: none ran. The registration names `algo-1.1.1+42d749f7c50b`.
- Compute: none (0 of 5,120 games, 0 of 172,000 simulated suites).

## What happens to the question

- The churn of the first real candidate is still measured, descriptively: EXP-0006's side-by-side run on EXP-0001's pilot players and the smoke suite reports, by checkpoint and by flaw, the reference hits the challenger loses and the misses it gains, with Jeffreys 95% intervals (`../EXP-0006/results.json`, `exploratory.churn`). It replaces nothing in EXP-0001's sizing.
- The rule for the next attempt, written now so it cannot be fitted to a result: the suites move down only on the churn of a candidate that changes a detector or the ranking (the changes the suites exist to catch), or on the largest churn over the candidates measured; a prior change's churn counts as a lower bound. The design needs about 83 reference hits for a "below" that can move the suites (the full suite's 110 flawed players and the smoke suite's 15 give about 80 at today's recall), so the re-registration runs on the full suite.

## Next

1. **Re-register the churn measurement on the first detector or ranking candidate**, on the full and smoke suites side by side (`ALGO_VARIANT`), with the fixes of review concerns 3 and 4 (the budget-bound branch computes its detectable change; the integrity checks EXP-0006 now has). Filed in `.github/settings/roadmap-issues.json` ("EXP-0024").
