# EXP-0007: decision

- Decision (section 9, per tag): 4 of the 10 motif tags are trusted, `fork`, `discoveredAttack`, `promotion` and `smotheredMate`, two of them (`promotion`, `smotheredMate`) close to pattern checks on the final move or position; the five mate-length tags pass as replay checks. `deflection` is hidden until fixed. `hangingPiece`, `pin`, `skewer`, `sacrifice` and `backRankMate` are hidden pending a human audit: the themes cannot tell their errors from their own gaps. No tag is "needs fixing".
- Algorithm: `algo-1.1.0+fa5e2186e1dd`. Results: `results.json` (commit 43f8be99), rerun to a byte-identical file in a fresh clone (`python -m rematch_lab.experiments check --history EXP-0007`, 0 problems); figure `figures/precision-recall.png`.
- Ground truth: Lichess's generated puzzle themes (CC0, many thanks to Lichess and its players), on puzzle solutions, over the 1,580,025 audience puzzles (puzzle rating below 2000, at least 1,000 plays, popularity at least 50). Not human labels, not engine lines, and not production's base rates. A band is a puzzle's rating, not a player's.
- Compute: 54,987 lines tagged in 28 cli calls, exactly the registered budget (cap 150%); no engine, no AI tokens. Each run took about 60 seconds of wall time in the lab container, most of it reading the 1.2 GB snapshot; the pilot about 30 seconds and the power simulation about a minute. Exclusions: none.

## The primary: precision against the floor of 0.80

Bonferroni intervals over 30 (precision and recall of 15 tags, two-sided 1 − 0.05/30), the linearised variance (section 8). Recall is the confirmatory secondary, against 0.50.

| Tag | Precision | Bonferroni interval | Recall | Bonferroni interval | Decision |
|---|---|---|---|---|---|
| `fork` | 0.976 | [0.969, 0.982] | 0.725 | [0.709, 0.740] | trusted |
| `discoveredAttack` | 0.992 | [0.985, 0.996] | 0.858 | [0.839, 0.875] | trusted |
| `promotion` | 0.998 | [0.987, 1.000] | 0.816 | [0.779, 0.848] | trusted |
| `smotheredMate` | 1.000 | [0.953, 1.000] | 1.000 | [0.953, 1.000] | trusted |
| `skewer` | 0.810 | [0.778, 0.838] | 0.897 | [0.867, 0.920] | hidden pending a human audit |
| `backRankMate` | 0.606 | [0.575, 0.636] | 1.000 | [0.989, 1.000] | hidden pending a human audit |
| `hangingPiece` | 0.572 | [0.550, 0.593] | 0.999 | [0.992, 1.000] | hidden pending a human audit |
| `deflection` | 0.562 | [0.527, 0.597] | 0.335 | [0.307, 0.363] | hidden until fixed |
| `sacrifice` | 0.528 | [0.513, 0.544] | 0.824 | [0.806, 0.840] | hidden pending a human audit |
| `pin` | 0.402 | [0.368, 0.436] | 0.201 | [0.181, 0.222] | hidden pending a human audit |
| `mate` | 1.000 | [0.999, 1.000] | 1.000 | [0.999, 1.000] | replay check passed |
| `mateIn1` | 1.000 | [0.997, 1.000] | 1.000 | [0.997, 1.000] | replay check passed |
| `mateIn2` | 0.999 | [0.997, 1.000] | 1.000 | [0.999, 1.000] | replay check passed |
| `mateIn3` | 1.000 | [0.996, 1.000] | 1.000 | [0.996, 1.000] | replay check passed |
| `mateIn4Plus` | 1.000 | [0.978, 1.000] | 1.000 | [0.978, 1.000] | replay check passed |

`matingNet` has no counterpart; it is on 19.7% of the audience puzzles.

What the numbers say, tag by tag:

- `skewer` is not shown to clear 0.80; its estimate, 0.81, is above the floor, and only the lower end of its interval hides it.
- `hangingPiece` and `backRankMate` find almost every puzzle Lichess labels (recall 0.999 and 1.000) and fire on many more: our rules are wider than the themes, which is not the same as wrong. `backRankMate`'s rule already asks that the king be shut in by its own pieces, so the gap is in which mates count, and the audit has to say whose definition the product wants. Almost none of `hangingPiece`'s false positives carry a motif theme (the largest are `defensiveMove`, 0.10, and `intermezzo`, 0.04).
- `sacrifice`'s false positives carry `attraction` (0.24), `discoveredAttack` (0.19) and `deflection` (0.14), ideas that usually involve giving material, which fits label gaps; its precision is lowest in the easiest band (0.36 below 1000, 0.66 above 2000).
- `pin` needs a rule fix whatever an audit finds: its recall, 0.20, is far below 0.50, and missing a labelled pin is no label gap. 24% of the puzzles we tag `pin` carry `deflection`, and 16% of Lichess's deflection puzzles get our pin: the two rules fire on the same overloaded defenders.
- `deflection` is our error, as the pilot's hand check found: its false positives carry `attraction` (0.16) and `fork` (0.13), and of Lichess's deflection puzzles we tag 33% `deflection` and 36% `sacrifice`.
- The mate-length tags disagree on a handful of puzzles (for example 5 puzzles we tag `mateIn2` that Lichess does not label); they are replay or label disagreements, not zero.

## Exploratory: by band

Recall falls as the puzzles get harder for every trusted motif: `fork` from 0.86 (below 1000) to 0.61 (1700 to 1999) and 0.54 above 2000; `discoveredAttack` 0.95 to 0.81 and 0.71; `promotion` 0.83 to 0.77 and 0.70. `fork`'s pooled recall is carried by the 1000 to 1699 bands' weight, and its hardest band sits near the floor. Precision falls with rating for `skewer` (0.90 to 0.77 and 0.71, so the two easiest bands would clear 0.80 on their own), `pin` and `backRankMate`. `smotheredMate` has no estimate above 2000: the pilot took all 24 puzzles of that cell, so it was not sampled, not zero. The per-band intervals are descriptive; at 2000 and above some cells are small.

The linearised effective sample size is larger than the count for some tags (`hangingPiece` precision 5,127 against 2,888 puzzles): the strata are defined by the labels, so cells where every tagged puzzle is a true positive add no variance, and fully sampled cells have no finite-population term. Section 6's simulation covers that at this design (coverage 0.996 at a true precision of 0.90). The mate tags fall back to Kish's effective size because their estimates are 1.

## Deviations and process

- None from sections 0 to 10 as frozen at commit fab55f92. That commit revised the design after the pre-run critique, 11 minutes after registration and before any confirmatory tag (`review.md`, part one); the revised design had no second independent critique before the run.
- Before the registration commit, a lab check ran the confirmatory analysis twice on the old sample; the outputs were deleted unread and the sample was redrawn since (`review.md`).
- The pilot's hand check was the agent's, not a person's, and not blind; it set only which hidden branch a tag takes.

## What happens next

1. The six hidden tags stop being named to players: "Stop naming the motif tags EXP-0007 did not clear", a major algorithm version with its ADR. Until it ships, the product still names them.
2. EXP-0019: a blind human audit of `hangingPiece`, `skewer`, `sacrifice` and `backRankMate`, against a definition the product writes down first.
3. EXP-0020: fix the deflection rule (attraction is not deflection; a king driven off by check).
4. EXP-0021: fix the pin rule (recall 0.20, the overlap with deflection).
5. EXP-0022: motifs after the first gain and past ply 8, from the recall that falls with the puzzle's rating.
6. EXP-0023: the tags of **Stockfish 19**'s best line against the solution's, so the KPIs can be read as production's.

All six are in `.github/settings/roadmap-issues.json` under "Epic: are the facts right?". The published KPIs read "against Lichess's generated puzzle themes, on puzzle solutions".
