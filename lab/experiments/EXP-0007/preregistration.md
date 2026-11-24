# EXP-0007: the motif tagger against the Lichess puzzle themes

- Status: pre-registered
- Issue: #102
- Author: the maintainer, with Claude (lane `exp/0007-motifs`)
- Kind: measurement (one version)
- Follows: none
- Registered: 2026-11-24. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: which motif tags Rematch may name to a player. The tags name the missed-motif series (`motif.missed:<tag>`, `backend/crates/profile-core/src/weakness/detectors.rs`, from the best line's tags), the motif of a drilled position after the attempt (`move_features.best_line.tags`, ADR 0090), and the coach's evidence (the exemplars' `tags`, `backend/crates/profile/src/coaching/exemplars.rs`).
- If the result is that a tag cannot be shown to clear the precision floor of 0.80, we will stop naming that tag to players (it is still computed and stored) until a fixed rule passes the same suite; if it clears the floor but cannot be shown to find half the motifs Lichess labels, we will keep naming it and fix its recall; if it clears both, it drives recommendations and drills as now, and its precision and recall become published KPIs.
- Expected cost: no engine and no AI tokens: `cli lab tag-motifs` replays each puzzle's own solution through the production feature derivation, CPU seconds to minutes on one core. A one-off download of the puzzle database (309,751,173 bytes). About a day of maintainer time, mostly the hand check of section 7.
- Value score: impact 3 × uncertainty 3 ÷ cost 1 = 9
- Cheaper alternative? No. The unit tests' hand-built lines (`backend/crates/profile-core/tests/motifs.rs`) were written by the authors of the rules, so they cannot measure them, and no other labelled set of real positions exists at this scale. The sample is the power analysis's, not the largest the data allow.
- Compute budget: 23,817 lines tagged by `cli lab tag-motifs` (the size of `sample.csv`, section 6) in 12 cli calls of at most 2,000 lines; no engine nodes, no profile calls, no AI tokens; cap 150%

## 1. Question

How often is a position Rematch calls a fork really a fork, and how many of the forks a human-checked label set contains does Rematch call a fork? The same for each of the 15 tags with a Lichess counterpart. A tag that is wrong one time in three would teach a 1400 player to drill "forks" that are not forks, and would inflate their `motif.missed:fork` series with positions of another kind.

## 2. Hypotheses

A measurement study: the hypotheses name the branches of section 9.

- H0 (per tag): the tag's precision against the Lichess themes is below 0.80, or its recall is below 0.50.
- H1 (per tag): precision is at least 0.80 and recall at least 0.50.

Rival explanations that would give the same numbers, and how each is handled:

- Label gaps. A Lichess theme names the solution's idea, not every motif in it: a fork puzzle whose solution also pins a piece carries `fork` and not `pin`, so a correct `pin` tag counts as a false positive. The pilot's hand check (section 7) measures the share of disagreements that are label gaps, per tag, before the run; it is reported as a caveat beside each precision and decides the kind of follow-up (section 9), never the decision itself. Precision against the themes is therefore a lower bound on precision against the truth, up to label errors in the other direction.
- Labels made by another tagger. The themes start from Lichess's own tagger and are then voted on by players. The population is restricted to puzzles played at least 1,000 times with a popularity of at least 50 (section 7), where the votes have had time to correct them.
- Truncated lines. A puzzle's solution stops when the win is clear; the engine's best line in production goes on. The swing (`settled_swing`, `SETTLE_EXTRA` 4) settles on the moves it has, so a capture that ends a puzzle is counted without the recapture that might follow. This can shift a swing's size and depth, and so the window the rules read. It is not measured here; extending each solution with **Stockfish 19** would cost engine time on 6 million candidates for a second-order effect.
- Puzzle positions are not game positions. Every puzzle has a winning solution; the product tags the best line of every critical move. Precision and recall here are per tactical position, not per production line, and the base rate of each motif differs.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-1.1.0+fa5e2186e1dd` | – |

From `cli algo show` on the cli built from this branch. The bench crate gains `bench::lab::tag_motifs` and the cli gains `cli lab tag-motifs`; neither crate is in the manifest, and the id is unchanged. The tagger is `backend/crates/profile-core/src/motifs/` (`SACRIFICE_PAWNS` 2, `SETTLE_EXTRA` 4, `MAX_REPLAY_PLIES` 128), called through `profile_core::features::derive::derive_move_features` with `LINE_MAX_PLIES` 8.

## 4. Primary metric and ground truth

- Primary metric (exactly one): the precision of each scored tag, the weighted share of the audience puzzles we tag with it that carry its Lichess truth label, with its Bonferroni interval (section 8), against the floor of 0.80.
- Ground truth: the themes of the Lichess puzzle database (CC0, many thanks to Lichess and its players), the snapshot of section 7. The truth label of each tag:

| Our tag | Lichess themes that count |
|---|---|
| `hangingPiece` | `hangingPiece` |
| `fork` | `fork` |
| `pin` | `pin` |
| `skewer` | `skewer` |
| `discoveredAttack` | `discoveredAttack`, `discoveredCheck`, `doubleCheck` (our rule includes a discovered check) |
| `deflection` | `deflection` |
| `sacrifice` | `sacrifice` |
| `promotion` | `promotion`, `underPromotion` |
| `mate` | `mate` |
| `mateIn1`, `mateIn2`, `mateIn3` | the same |
| `mateIn4Plus` | `mateIn4`, `mateIn5` |
| `backRankMate` | `backRankMate` |
| `smotheredMate` | `smotheredMate` |

  `matingNet` has no counterpart and is not scored; its tagged share is reported. Lichess motifs with no tag of ours: `attraction`, `capturingDefender`, `clearance`, `interference`, `intermezzo`, `xRayAttack`, `zugzwang`, `trappedPiece`, `quietMove`, `defensiveMove`, `exposedKing`, `kingsideAttack`, `queensideAttack`, `advancedPawn`, `attackingF2F7`, `enPassant`, `castling`, and the named mates (`anastasiaMate`, `arabianMate`, `bodenMate`, and the rest).
- Secondary metrics:
  - confirmatory: the recall of each scored tag, the weighted share of the audience puzzles carrying its truth label that we tag with it, with its Bonferroni interval, against the floor of 0.50; it decides "trusted" against "needs fixing" (section 9), never whether a tag is shown;
  - exploratory: precision and recall per rating band, the slice above (2000 to 2399) included, with 95% intervals and no correction;
  - exploratory: the confusion between related tags, the share of each tag's puzzles carrying each Lichess motif theme and the share of each theme's puzzles carrying each of our tags (pin against skewer and `xRayAttack`, deflection against `attraction` and `capturingDefender`, discovered attack against `discoveredCheck`, the mate patterns);
  - exploratory: the pilot's label-gap share per tag (section 7).

## 5. The minimum effect that matters

- Minimum effect: 0.10 from the precision floor and 0.15 from the recall floor. A tag whose true precision is 0.90 must be shown, and one at 0.70 hidden; a tag whose true recall is 0.65 must be found trusted.
- Why this size: the floors are product rules. A precision of 0.80 is one named motif in five that is not there, the most a player can be shown before the drill labels lose their meaning; between 0.70 and 0.90 the product's choice turns. A recall of 0.50 means the missed-motif series counts at least half the player's misses; below it the series under-states a weakness by more than half, which is the size at which the ranking (excess over the band median) can drop it. A recall difference of 0.15 is coarser than the precision one because recall decides a fix, not what a player sees.

## 6. Power analysis

- Method: a simulation of the section 9 rule under the section 7 design (`analysis.py power`), 2,000 replicates a size, seed `power/simulation`, two-sided Bonferroni intervals over the 30 intervals (z = 3.14).
- Inputs: the population's cell sizes and label shares (exact, `strata.csv`); each tag's rates of tagging a labelled and an unlabelled puzzle in each stratum, from the pilot (development data, smoothed as (k + 0.5)/(n + 1)); for the precision scenario the false-positive rates are scaled so that the population precision is 0.90, for the recall scenario the true-positive rates are scaled (capped at 1) so that the recall is 0.65.
- Result: the allocation n = min(N, max(m, ⌈f·N⌉)) a cell of N puzzles, with m = 200 and f = 0.01: 19,870 audience puzzles (23,817 with the slice above, `sample.csv`). It is the smallest design on the grid m ∈ {50, 100, 150, 200, 300} × f ∈ {0, 0.005, 0.01, 0.02, 0.03, 0.05}, ordered by its audience puzzles, whose worst tag reaches 0.80: the worst is `smotheredMate`'s precision, 0.84 (95% Monte Carlo ±0.016); `pin` 0.93, `deflection` 0.96, every other tag at least 0.98 for precision, and every tag 1.00 for recall. The next smaller design (m = 150, f = 0.01, 18,293) reaches 0.80 for its worst tag only at 0.799. A tag whose true precision is 0.70 is shown in none of the 2,000 replicates for any tag (the false-show rate), at this design.
- Equal allocation (f = 0) is not enough at any m on the grid: a cell of the `none` stratum holds up to 223,180 puzzles, so one of its puzzles weighs about a thousand times one of a rare stratum's, and a single false positive there moves a precision by several points. The fraction f bounds the weights at 1/f.
- Sensitivity: the power of every tag at every design of the grid (`power.json`, `figures/power.png`). The simulated coverage of the Bonferroni precision interval at a true precision of 0.90 is at least 0.99 for 12 tags, 0.94 for `mateIn4Plus` and 0.86 for `smotheredMate` (nominal 0.998): Kish's effective sample size understates the variance when a few heavy-weight false positives decide a rare tag's precision. For those two tags a "trusted" verdict carries that caveat. The scenario puts the false positives where the pilot's smoothed rates put them, which for a tag with no pilot false positive is an assumption ((0 + 0.5)/(n + 1) in every stratum).
- The pilot (`pilot/pilot.json`) is 1,874 puzzles, 25 a cell (24 in the one cell with fewer), all tagged, none excluded.

## 7. Design

- Unit of analysis: the puzzle. Each comes from a different game (6,193,530 puzzles, 6,193,530 distinct game URLs in the snapshot), so no two units share a game.
- Population: the puzzles of the snapshot played at least 1,000 times with a popularity of at least 50 and a rating below 2400.
- Strata and blocks: rating bands A1 (below 1000), A2 (1000 to 1399), A3 (1400 to 1699), A4 (1700 to 1999), the audience; and S (2000 to 2399), the slice above. Within each band, a puzzle's stratum is its rarest scored truth label in the population (ties by name), or `none` when it has none. A cell is a stratum in a band.
- Sample: min(N, max(m, ⌈f·N⌉)) puzzles of a cell of N (the population less the pilot), at random within the cell, with m = 200 and f = 0.01 (section 6); 23,817 puzzles in 75 cells (`sample.csv`). Each puzzle weighs its cell's N over its cell's sample, so every estimate is a population estimate over the audience (Horvitz-Thompson).
- Seeds: `sample/pilot`, `sample/confirmatory`, `pilot/hand-check`, `power/simulation` (`rematch_lab.experiments.seed`); no other source of randomness.
- Development and confirmatory data: the pilot, 25 puzzles a cell (seed `sample/pilot`), is development data: it sets the power analysis and the hand check, and is excluded from the confirmatory sample. The decision rule, the floors and the minimum effects (sections 4, 5 and 9) were committed before the pilot ran.
- The hand check: from the pilot, seeded (`pilot/hand-check`), up to 4 puzzles a tag that we tag and Lichess does not label, and 2 that Lichess labels and we do not tag, each judged by hand from the position and the solution: a label gap (the motif is in the solution, Lichess did not name it), a tagger error, or unclear. The verdicts are in `hand-check.md`. A tag whose checked false positives are at least half label gaps is label-limited (section 9).
- Held fixed: the snapshot, the filters, `LINE_MAX_PLIES` 8, the cli of section 3.
- Data: the Lichess puzzle database, `https://database.lichess.org/lichess_db_puzzle.csv.zst`, downloaded 2026-11-24 01:09 UTC (Last-Modified 2026-11-01 07:57:40 GMT), 309,751,173 bytes, sha256 `7b0d235fa40cfd7fdb7df1fec56a96b3b6ee1496b32ed2eb05be90d0ebf94253`; decompressed with zstd 1.5.7 to `lichess_db_puzzle.csv`, 1,177,486,537 bytes, sha256 `8ce98180bcb7cb60f3f51f12afdbe6a4fd294d8aaf4f1410e8c25e85ac78fdc9`, the external input of `inputs.sha256` (in `data/lichess-puzzles/`, git-ignored; Lichess replaces the file every month, so the snapshot is kept by its checksum). Only the sample's ids (`sample-pilot.csv`, `sample.csv`) and the cell counts (`strata.csv`: each cell's population, its pilot and the population count of each truth label) are committed, with their sha256 in `inputs.sha256`. The population is 1,826,817 puzzles (1,580,025 in the audience). No real chess.com player's games.
- Leakage controls: the pilot and the confirmatory sample are disjoint; nothing the confirmatory run computes feeds back into the design.
- Reuse: none; no engine evaluation is needed.

## 8. Analysis plan

- Model or test: no model. Each estimate is a weighted share over the sampled audience puzzles (section 7's weights), with a Wilson interval at Kish's effective sample size, n_eff = (Σw)² / Σw², of the puzzles in its denominator.
- Effect size: precision and recall of each scored tag with 80%, 95% and Bonferroni (two-sided, 1 − 0.05/30, z = 3.14) intervals; the decision reads the Bonferroni intervals.
- Multiple comparisons: Bonferroni over the 30 intervals of the decision (precision and recall of 15 tags). The per-band and confusion figures are exploratory and uncorrected.
- Diagnostics: no fitted model. The power analysis reports the simulated coverage of the Bonferroni precision interval at a true precision of 0.90.
- Exclusions and missing data: a puzzle whose setup move or solution the production code cannot play gets no analysis (`best` null); it is excluded and listed in `results.json`. A run that spans more than one `algorithm_id` stops.

## 9. Decision rule

Measurement, per tag, replacing the template's band rule: the action for a tag that cannot be shown to clear the floor is the same as for one shown below it (it is not named to players), so there is no "need more data" branch.

- Trusted: the Bonferroni interval of precision lies wholly at or above 0.80, and that of recall wholly at or above 0.50. The tag drives recommendations and drills as now.
- Needs fixing: precision as above, recall not. The tag is still named; a follow-up experiment issue is filed under "Next" for a rule that finds more of its motifs.
- Hidden until fixed: the lower end of the precision interval is below 0.80, or we never tag it. The tag is computed and stored but not named to players (the missed-motif series, the drill's motif line and the coach's evidence) until a fixed rule passes this suite. The follow-up issue is a rule fix, or, when the tag is label-limited (section 7), a hand-labelled audit first.

### Stopping rules

- Early stopping: none. The run is one pass over a fixed sample, minutes of CPU.
- Futility: none, for the same reason.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0007/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`; its `sample`, `pilot` and `power` stages ran before registration, in that order)
- Analysis code sha256: `0ddc38369fc9cbf908929fa006280d5da7af97e23513a867ef1714d1ef2fc701`

## 11. Deviations

None.
