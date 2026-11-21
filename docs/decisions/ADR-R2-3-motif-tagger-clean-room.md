# ADR-R2-3. Clean-room motif tagger

Date: 2026-11-21
Status: Accepted (R2 Task 5, `profile-core::motifs`). Placeholder number; the controller renumbers R2 ADRs before merging.

## Context

The profile engine tags engine lines (refutation, missed and threat lines, improvement-engine §4.4) with tactical motifs, so weaknesses can say "you allowed a fork" or "you missed a mate in 2". The best-known tagger, the Lichess puzzle tagger `cook.py`, is AGPL-licensed. The licensing rules of this project (global constraints, TS M2 §14) forbid copying or porting it, or any other copyleft chess code (lila, chessops, shakmaty, python-chess).

## Decision

1. **Clean room.** The rules are our own, written from the published one-line descriptions in improvement-engine §4.4 and the rule wording of TS M2 Task 4. No code, structure or thresholds come from `cook.py` or any other copyleft source; none was read while writing this. The implementation sits on our own board layer (R2 Task 4) and cozy-chess (MIT).

2. **Interface.** `tag_line(fen, pv, pov, max_plies /* 8 */) -> LineAnalysis`. The swing is Task 4's `settled_swing` with `settle_extra = 4`. The whole PV is replayed, because `depth` can exceed `max_plies` for a deep mate, but only its first `MAX_REPLAY_PLIES = 128` moves: later moves are ignored for the swing and for every tag, which bounds the work per line (engine PVs are far shorter). Every index uses `get`/`take`, never a range slice that could panic.

3. **No swing, no tags.** When `swing.depth` is `None`, the tag list is empty. Otherwise the rules see only plies `≤ depth + 1` (the "window"), with two deliberate exceptions:
   - **Mate exception.** The mate rules (`mate*`, `matingNet`, `backRankMate`, `smotheredMate`) look at the mating step at the end of the replayed PV regardless of the window, because an early material swing can set `depth = 1` while the mate comes much later.
   - **Fork exception** (controller ruling, Task 5 review). The fork move itself must lie in the window, but the capture that completes the fork may come from anywhere in the settled replay: the first `max(window, max_plies + settle_extra + 1)` plies. A capture fork (Nxd6+ winning a rook, forking king and queen) or a promotion fork (e8=N+) already realises the swing at ply 1, so `depth = 1` and the completing capture at ply 3 lies outside the window.

4. **Tag list** (`MotifTag` declaration order, which is also the output order, without duplicates): `hangingPiece`, `fork`, `pin`, `skewer`, `discoveredAttack`, `deflection`, `sacrifice`, `promotion`, `mate`, `mateIn1`, `mateIn2`, `mateIn3`, `mateIn4Plus`, `matingNet`, `backRankMate`, `smotheredMate`. Eleven rules, one function each in `motifs/rules.rs`:
   - `hangingPiece`: pov's first ply captures a knight, bishop, rook or queen that no non-king piece defended before the capture (`defenders_of` on the position before the move), and no **legal** move then lands on that square. A king recapture into a guarded square does not count, and a piece defended by another piece is not hanging even when a double check stops the recapture. A mating capture of a piece defended only by the king counts as `hangingPiece` (no legal recapture exists), alongside the mate tags.
   - `fork`: after a pov move, the moved piece attacks at least two enemy non-pawn pieces, each the king, worth more than the mover, or undefended; the mover is not en prise; and **the forking piece itself** (`later.from` is the fork square) later captures on a target square, within the settled replay (fork exception above). A different piece capturing a "target", for example because the forker is pinned, is no fork. **Timing (R2 Task 6, from the Task 5 re-review):** the completing capture must be the forker's first move after the fork (no ply in between moves from or lands on the fork square), or the captured target must not have moved since the fork (no ply in between moves from or lands on its square). A queen that dodges and later walks back into a knight that has moved away and back is therefore no fork: the capture was won by the opponent's later error, not by the double attack. A target that dodges and returns while the forker waits still counts, because the forker's first move is the capture. Pawns are not fork targets, following §4.4's "opponent non-pawn pieces".
   - `pin`: the piece captured at the swing ply stood pinned (`pinned_to`, so by a pov slider, to its king or a more valuable piece) on that square in an earlier position with the opponent to move.
   - `skewer`: a pov slider captures along a line through a square that a more valuable enemy piece (the king outranks everything) vacated on the previous ply. §4.4 adds that the piece "was forced to move"; we drop that condition and do not prove the move was forced.
   - `discoveredAttack`: a pov move vacates a square, and a different pov slider then attacks through it the enemy king (the move gives check) or an enemy piece that the slider captures two plies later.
   - `deflection`: a pov capture or check at ply k; the opponent's reply at k+1 moves piece D to t and either recaptures on ply k's square or gives check; at k+2 pov captures on a square x ≠ t that D defended before k+1. A voluntary reply (no recapture, no check) is no deflection. We still do not prove the reply was the only move.
   - `sacrifice`: some pov ply in the window has `settled(k) ≤ −2` against the starting balance, the line still gains at least 2 or mates, and the opponent does not promote in the window. `settled` comes from Task 4's `settled_plies`, whose recapture chain stops before a pov mate, so a mating sacrifice reads as −5.
   - `promotion`: a pov move in the window promotes.
   - `mate`: `mate` plus `mateIn1`, `mateIn2`, `mateIn3` or `mateIn4Plus` from `mate_in`; `matingNet` when `mate_in ≥ 2`.
   - `backRankMate`: a rook or queen gives the mate from the mated king's back rank, the king stands on that rank, and every forward escape square holds one of the king's own pieces.
   - `smotheredMate`: a knight gives the mate and every square next to the king holds one of the king's own pieces.

5. **Shared definitions** (from the Task 4 review):
   - **En prise** means "attacked, and undefended or attacked by a cheaper piece". The source is §4.4's fork rule ("the mover isn't en prise"). The attack queries are pseudo-legal and direct only (chess.js `attackers` parity): no x-rays, pins or counts.
   - **`gain` is 0 when a line has no pov plies.** TS `Math.max()` over an empty list would give −Infinity; we never emit it.

6. **Two spellings of a tag.** Internally and in TS-parity JSON (`LineAnalysis.tags`), tags use the TS camelCase keys (`hangingPiece`, `mateIn2`). The M3 contract (`ExemplarPosition.tags`) uses snake_case (`hanging_piece`, `mate_in_2`), produced only by `MotifTag::contract_tag` at the contract boundary.

7. **Departure from §4.4's hanging-piece wording.** §4.4 reads "captures an undefended non-pawn piece and the opponent can't recapture with an equal or higher-value piece". We use two tests instead: no non-king defender before the capture, and no legal recapture at all, by any piece, afterwards. A recapture by a cheaper piece therefore also stops the tag, so a piece that can be taken back is never called hanging. The king's defence is ignored before the move and checked for legality after it.

## Consequences

- The rules are deliberately small and may miss motifs that cook.py's richer detectors catch (x-ray attacks, interference, attraction, and others). Each new motif or refinement is a separate decision.
- **Pseudo-legal queries.** The fork, deflection and hanging-piece "defended" tests count a defender that is itself pinned as a real defender. So a fork target can look defended (false negative), and a deflection can rest on a pinned defender (false positive).
- **Pin.**
  - False positives: the rule checks only that the captured piece stood pinned on its capture square in an earlier position, not that the pin is why it was lost. A piece lost to some other tactic, which happened to be pinned earlier, is still tagged `pin`.
  - False negatives: a pinned piece that moves off its square before it is captured; a pin that wins the piece behind the pinned one rather than the pinned piece itself; a pin that exists only in positions with pov to move.
- **Discovered attack and en passant.** The vacated square is the moving piece's origin. An en-passant capture also vacates the captured pawn's square, and a line opened through that square is not seen (false negative).
- Pinned by `backend/crates/profile-core/tests/motifs.rs`:
  - the TS M2 Task 4 assertions and an exact tag set per fixture;
  - a mate in 6 past the horizon, and a late back-rank mate after a depth-1 material swing;
  - legal-recapture and double-check hanging-piece cases, and a mating capture that counts as hanging;
  - an en-prise fork, and a pinned forker whose target another piece takes; neither is a fork;
  - fork timing: a queen that dodges and comes back after the knight moved away and back is no fork; the same dodge with a waiting knight, and an unmoved target after the knight moved, are forks;
  - capture and promotion forks completed past the window;
  - a voluntary reply that is no deflection;
  - the 128-ply replay cap.
