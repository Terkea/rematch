# 0117. A trajectory verdict that counts the rating's memory

Date: 2026-11-27. Status: accepted (lane `exp30`, branch `exp/0030-memory`, EXP-0030). Builds on ADRs 0083 §6, 0089, 0104, 0105, 0109 and 0116, which it partly lifts. Ships as `algo-6.0.0`.

## Context

ADR 0116 hid the trajectory's rating verdict because EXP-0015 found it told 44% to 78% of simulated players whose strength never changed that their rating was falling. Two things were wrong: the bootstrap treated 30 consecutive pre-game ratings as 30 independent ones, while a rating carries most of its error from one game to the next (lag-1 correlation 0.97), so its interval was six to thirteen times too narrow; and "then" sat on the peak, a maximum of the same noise.

EXP-0030 registered two candidates that fix both, froze them, drew its confirmatory seeds from a public beacon after the freeze, and decided by its registered rule (`lab/experiments/EXP-0030/decision.md`). On 2,000 fresh constant-strength players `memory-halves` said "falling" to 0.103, 0.117 and 0.105 and "rising" to 0.089, 0.084 and 0.095 at 60, 120 and 250 rated games, every cell within 0.15. It caught a 200-point fall 0.72, 0.84 and 0.99 of the time. It passed:
- the rough-world guard (sessions, breaks, form and tilt; 0.125 to 0.144);
- the chess.com-like guard (0.086 to 0.107);
- the real-cohort guard, though there 36% of settled real Lichess players were flagged either way against the model's nominal 20%.

`memory-before` cleared too. The registered order ships `memory-halves`.

## Decision

1. **The rule is `memory-halves`** (`profile::coaching::trajectory::RatingRule::PRODUCTION`), in Rust and in the web's `computeTrajectory`, kept in parity by `backend/fixtures/trajectory/cases.json`.
   - "Then" is the first `W = min(30, floor(n / 2))` rated games and "now" the last `W`; no window is placed on the peak.
   - The rating row's 80% interval comes from the player's own steps, read as a mean-reverting rating (`Memory`). Let `s` be the root mean square of consecutive pre-game rating changes and `q = ln 10 / 400`. Then `a = min(q s / 2, 1)`, `φ = 1 − a` and `σ² = s² / (a (2 − a))`.
   - The variance of the difference of the two block means is exact under that model. The verdict is called when `diff ± 1.2816 √Var` excludes 0 (ADR 0083 §6). A series whose rating never moved gets no verdict.
   - The accuracy, blunders, score and weakness rows keep their rules, now over the first and last windows.
2. **`change` is the difference the verdict tests**: the last window's mean minus the first window's, never the current rating against the peak. The peak stays a plain fact.
3. **The trajectory names its rule.** The contract's `Trajectory` gains an optional `ratingRule` (`memory-halves`), absent on every input built before this version. The coaching input stays `coaching-input-v3`; a stored v3 input still parses.
4. **The switch turns on, for the cleared rule only.**
   - `coaching_input::trajectory::RATING_VERDICT_SHOWN` is `true` and `RATING_VERDICT_SOURCE` is `"EXP-0030"`; `coaching-input/tests/trajectory.rs` pins both to EXP-0030's `results.json`.
   - `CLEARED_RULE` is `"memory-halves"`, and the manifest names it (`profile.trajectory_rating_verdict.rule`).
   - `player_visible` passes a trajectory whose `ratingRule` is the cleared rule and cuts any other exactly as ADR 0116 did. So a stored input from `algo-5.x` or earlier, whose verdict came from the old rule, still reaches the coach withheld.
5. **The words.**
   - **Web headline.** Progress and the Overview card say: "Your rapid rating averaged 1,492 over your last 30 games, down from 1,560 over your first 30 (−68). It is 1,488 now. Your highest was 1,610, in April 2026." "Up from" applies to a rise, and a level line adds "no clear change". The highest rating is mentioned only when it is above the current one. "Peaked at … (−N)" is gone: it compared a maximum with one draw, so it was negative for a stable player whatever the rule.
   - **Web table.** The then | now table's rating row shows its verdict and its 80% ranges again, and its "What is this?" says the range counts the rating's memory and that the verdict says the rating moved, not why. With the switch off, the same headline names no direction.
   - **The coach.** It reads `report/report.v6.md` and `chat/system.v8.md`. They give the change between the averages and the peak as a fact with no difference against it, and say the verdict means the rating moved by more than its own wander, not that the player's chess got better or worse. An older input's `withheld` keeps v5's plain words.
6. **The lab keeps the old rule.** `peak-bootstrap` and `memory-before` stay selectable through `cli lab trajectory`'s `rule`. EXP-0015's arena now measures `memory-halves` on EXP-0015's registered seeds. EXP-0030 is live (`rerun: release`) with its own `arena.py`, which compares a build's `memory-halves` flags.

## Options considered

1. **Ship `memory-before`**, which did better in some reads that do not gate. Rejected by the registered order, which put the simpler statement first ("your first 30 games against your last 30"): its "then" does not grow with the history. The trade-off is recorded in `decision.md`:
   - the rough world's worst cell: 0.141 for `memory-before` against 0.144;
   - detection at 120 games: 0.88 against 0.84;
   - new accounts at 250 games: about 0.03 against 0.17.
2. **Keep the verdict hidden until the real-series question is answered.** Rejected: the registered rule decided, and the real check passed its registered line. Its 36% stays visible in the decision and is EXP-0032's question.
3. **Turn the switch on for every input.** Rejected: inputs stored before `algo-5.0.0` carry the old rule's verdict uncut, and the coach still reads them. The rule name makes the cut exact without guessing from the numbers.
4. **A new coaching-input version.** Rejected for the same reason as in ADR 0116. One optional field is added, a stored v3 input still parses, and the algorithm's major version names the change.

## Consequences

- A player sees a rating verdict again on the next page load, and the coach on the next report or chat turn built from a new input. Stored inputs stay withheld until rebuilt.
- The claim is bounded by what EXP-0030 measured:
  - settled simulated series of 60 to 250 games;
  - a guarded chess.com-like world with assumed constants;
  - a real check that bounds only gross failure.
- Production reads full histories, often longer and starting in the provisional period, where `memory-halves` already reads 0.17 to 0.18 from a new account at 250 games. EXP-0031 tests that and decides whether a settled-history gate is needed. EXP-0032 measures the model on real series at the product's windows.
- Even a calibrated rule leaves many early headlines false. If a fifth of players fell 200 points, about 37% of "falling" headlines at 60 games would be false. The copy says what the verdict means and names no cause.
- The replay goldens change only in their header. The lab's trajectory golden changes with the rule.
- EXP-0001's arena guard sees `backend/crates/bench/src/lab_rules.rs` change. The change adds an optional `rule` to `cli lab trajectory`, which is the production rule when absent. EXP-0001 runs only `rematch_lab.bench(suite_file=…)` and never calls `cli lab trajectory`, so it takes exactly the statements it took before. The file's blob at algo-6.0.0 is reviewed for EXP-0001 in `lab/source-reviews.json` (ADR 0109 Amendment 1).
