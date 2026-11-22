# ADR-R2-4. Weakness metric rulings (detectors W1-W12)

Date: 2026-11-22
Status: Accepted (R2 Task 8, `profile-core::weakness::{detectors, observe_game, tilt}`). Placeholder number; the controller renumbers R2 ADRs before merging.

## Context

Improvement-engine §6.2 defines the weaknesses W1-W12 in prose. TS M2 Task 7 (ADR 0037 in the TS plan) turned that prose into a per-move opportunity/event table. R2 ports the table to Rust and adds the R2 honesty rules: nothing is claimed from zero evidence, every denominator counts only computable opportunities, invalid inputs are skipped and counted, and no NaN or infinity reaches the output. It also carries the hand-offs from Tasks 4 and 6: lines (`best`, `refutation`, `threat`) and `threat_addressed` are `None` when no line exists, and move difficulty comes only from Maia.

## Decision

### Rulings carried from TS M2 Task 7

1. **Sub-series.** W3 splits into `W3.shallow` and `W3.deep`, W4 into `W4.missed` and `W4.allowed`, W7 into `W7.traps` and `W7.prep`, and W8 into `W8.basics` and `W8.advanced`. Each is its own binomial key, posterior and series. §6.2's ratios and means become binomial counts as follows:
   - **W10** counts events (ΔW ≥ 5) over quiet eligible middlegame moves. Each event's ΔW still becomes EPL.
   - **W9b** counts decisive errors over eligible moves with the clock under 30 s, in bullet, blitz or rapid. The band comparison replaces §6.2's ratio of the low-clock rate to the high-clock rate.
   - **W9c** counts a mistake or blunder played in under half the player's median time, over eligible critical moves with a time reading.
2. **W9d and W12 are continuous and never ranked.**
   - W9d is the per-game Spearman ρ of time spent against the MultiPV gap.
   - W12 is the residual (actual minus expected score) of "tilted" games against the rest. "Tilted" means `index_in_session ≥ 4` or right after a loss in the same session (`tilt_residual_difference`).
3. **Depth 4 counts as deep.** §6.2 says shallow is "≤ 3" and deep is "≥ 5", which leaves depth 4 out. We count `depth ≤ 3` as shallow and `depth ≥ 4` as deep.
4. **W6 event cost is 0.5**, a fixed half point per losing episode that was lost. W5's EPL is `1 − score`.
5. **W1 uses the settled swing as its SEE stand-in.** W1 fires when the refutation wins ≥ 2 pawns of material (Task 4's settled swing) within ≤ 3 plies and is not a mate. It does not compute a static exchange evaluation.
6. **Tablebase-eligible means ≤ 5 men**, kings included (the local Syzygy 3-4-5 set). A WDL pair on a larger position is invalid input.
7. **`T_win` is fixed at 85** (`observe_game::T_WIN`) until the per-band fit of §4.3 exists. A `t_win` that is not finite or lies outside (50, 100] finds no winning episode, and the run reports `invalid_t_win`.
8. **EPL is attributed once per move** by `EPL_PRECEDENCE` (W4 > W2 > W1 > W3 > W8 > W7 > W10). The first key in that list that claims a move takes its ΔW/100, and later keys get 0 for that move. The other keys keep their own costs:
   - W9a, W9b, W9c and W11 keep the ΔW of their own event moves;
   - W5 and W6 keep their game-level costs.

   W7.prep claims the player's opening-phase moves when its event fires, so its EPL is the opening loss that no higher key claimed. W9a's EPL is the player's ΔW from the first low-clock ply on.

### R2 honesty rulings

9. **Opportunities are moves whose event can be decided.** A missing input counts as unknown, never as a non-event. When the missing value cannot change the answer, the move stays an opportunity.
   - **W1 and `motif.allowed:*`:** an eligible move without a refutation line is an opportunity only if it is not decisive. A non-decisive move is never a W1 event, whatever its line, while a decisive move without a line cannot be judged.
   - **W2:** opportunities need `threat_existed == Some(true)`. A move whose `threat_addressed` is `None` counts only if it is not decisive.
   - **W3, W10 and W11:** these define their opportunities by the line or the difficulty itself. A move with `best` or `difficulty` set to `None` is never one of their opportunities.
   - **W8:** both WDL values must be present.
   - **W9:** W9a and W9b need a valid clock, and W9c needs a valid time spent.
10. **W3 and W10 need complete best-line coverage.** Enrichment fills a missing PV only on critical plies (R2 Task 25: ΔW ≥ 5, or after a ΔW ≥ 10 ply). In a game whose evals came from Lichess without PVs, the moves that have a best line were therefore selected on their outcome, and a denominator built from them would push the W3 and W10 rates towards 100%. If any eligible own move lacks its best line, the game reports no W3, no W10 and no `motif.missed:*`, and counts the missing lines in `moves_without_best_line`. Games analysed by our own Stockfish have a PV on every ply and are unaffected.
11. **MultiPV-only flags keep TS's "≠ true".** W3 requires `in_top2 != Some(true)`, and W10 requires `is_critical != Some(true)`. MultiPV runs only on critical plies, so requiring `Some(false)` would select on the outcome in the same way as ruling 10.
12. **W9d never substitutes a gap.** TS used `multipvGap ?? 0`. R2 uses only own moves that have both a time spent and a MultiPV gap, needs at least 10 of them, and otherwise reports `None`. W9d uses `stats::correlation::spearman` from Task 11, which merged before this task. The plan asked for a local helper only to avoid waiting on Lane P3.
13. **Difficulty is Maia only.** W11 passes every value through `features::maia_difficulty` again. A non-Maia model or an invalid probability is skipped and counted (`invalid_difficulty`). Plies ≤ 10 have no difficulty. Nothing is ever substituted.
14. **Game-level keys need their evidence.** Each of these keys gets its single opportunity only when its condition holds; otherwise the key is absent.

    | Key | Opportunity requires |
    |---|---|
    | W5 / W6 | a known result (1, 0.5 or 0) and the episode |
    | W7.traps | at least one valid own move by ply 20 |
    | W7.prep | a valid last opening ply |
    | W9a | a clocked, non-daily game (`in_trouble` known) |

    The `endgame:*` and `opening:*` series also need a known result.
15. **Series.**
    - Every present binomial key gets a series row with the same counts and EPL.
    - `motif.missed:<t>` covers the W3 opportunities (both depths) whose best line carries tag t.
    - `motif.allowed:<t>` has the W1 opportunity count for every tag, with events where a decisive move's refutation carries t. A tag with zero events still records how many times it could have happened.
    - `endgame:<type>`: the opportunity is the first endgame ply with the player's W ≥ 30. The event is "not won" from W ≥ 70, or "lost" from 30 ≤ W < 70. The EPL is the player's endgame ΔW when the event fires.
    - `opening:<line_key>`: opportunity 1, event "lost", and the EPL is the player's opening ΔW when lost.
16. **Invalid inputs are skipped and counted** (`ObservationScan`):
    - moves (either colour) with a non-finite or out-of-range W, ΔW or score;
    - clocks or times spent that are non-finite or negative;
    - out-of-range or oversize tablebase values;
    - invalid difficulty;
    - an unknown result.

    Moves are read in ply order whatever the input order. Every EPL is finite.

### Side effects of Task 4's whole-PV mate detection

Task 4 detects mates on the whole PV, so `mate_in` is set even for a mate far down the line.
- **W1** drops a refutation that wins material early and then mates deep. The `mate_in == None` condition removes it, even though a piece was hung.
- **W10** drops positions whose best line ends in a deep mate, because the swing then has a depth, so such a position is not "quiet".
- **"Deep mates count as W3.deep"** holds only when no settled swing ≥ 2 appears at k ≤ 3. An early material swing sets a shallow depth, and the move counts as W3.shallow.

## Consequences

- Lichess-eval games without PVs contribute no W3 or W10 evidence until they are re-analysed with PVs. This is less data, but no rate is inflated.
- W1 and W2 rates on such games stay unbiased. Decisive moves always get a PV, because they are critical plies.
- Task 9 can reuse `observe_game::win_at_opening_end` and the detector constants.
- Tests: `backend/crates/profile-core/tests/detectors.rs` covers every key with a positive case, a negative case and a no-opportunity case, plus the honesty rules.
