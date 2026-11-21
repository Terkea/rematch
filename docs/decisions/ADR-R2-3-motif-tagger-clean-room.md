# ADR-R2-3. Clean-room motif tagger

Date: 2026-11-21
Status: Accepted (R2 Task 5, `profile-core::motifs`). Placeholder number; the controller renumbers R2 ADRs before merging.

## Context

The profile engine tags engine lines (refutation, missed and threat lines, improvement-engine §4.4) with tactical motifs, so weaknesses can say "you allowed a fork" or "you missed a mate in 2". The best-known tagger, the Lichess puzzle tagger `cook.py`, is AGPL-licensed. The licensing rules of this project (global constraints, TS M2 §14) forbid copying or porting it, or any other copyleft chess code (lila, chessops, shakmaty, python-chess).

## Decision

1. **Clean room.** The rules are our own, written from the published one-line descriptions in improvement-engine §4.4 and the rule wording of TS M2 Task 4. No code, structure or thresholds come from `cook.py` or any other copyleft source; none was read while writing this. The implementation sits on our own board layer (R2 Task 4) and cozy-chess (MIT).

2. **Interface.** `tag_line(fen, pv, pov, max_plies /* 8 */) -> LineAnalysis`. The swing is Task 4's `settled_swing` with `settle_extra = 4`. The whole PV is replayed, because `depth` can exceed `max_plies` for a deep mate; every index uses `get`/`take`, never a range slice that could panic.

3. **No swing, no tags.** When `swing.depth` is `None`, the tag list is empty. Otherwise the rules see only plies `≤ depth + 1` (the "window"). The mate rules (`mate*`, `matingNet`, `backRankMate`, `smotheredMate`) look at the mating step at the end of the whole PV regardless of the window, because an early material swing can set `depth = 1` while the mate comes much later.

4. **Tag list** (`MotifTag` declaration order, which is also the output order, without duplicates): `hangingPiece`, `fork`, `pin`, `skewer`, `discoveredAttack`, `deflection`, `sacrifice`, `promotion`, `mate`, `mateIn1`, `mateIn2`, `mateIn3`, `mateIn4Plus`, `matingNet`, `backRankMate`, `smotheredMate`. Eleven rules, one function each in `motifs/rules.rs`:
   - `hangingPiece`: pov's first ply captures a knight, bishop, rook or queen, and no **legal** move then lands on that square. This is a legal-move test, not `defenders_of`: a king recapture into a guarded square does not count.
   - `fork`: after a pov move, the moved piece attacks at least two enemy non-pawn pieces, each the king, worth more than the mover, or undefended; the mover is not en prise; a later pov move in the window captures on a target square. Pawns are not fork targets, following §4.4's "opponent non-pawn pieces".
   - `pin`: the piece captured at the swing ply stood pinned (`pinned_to`, so by a pov slider, to its king or a more valuable piece) on that square in an earlier position with the opponent to move.
   - `skewer`: a pov slider captures along a line through a square that a more valuable enemy piece (the king outranks everything) vacated on the previous ply.
   - `discoveredAttack`: a pov move vacates a square, and a different pov slider then attacks through it the enemy king (the move gives check) or an enemy piece that the slider captures two plies later.
   - `deflection`: a pov capture or check at ply k, an opponent piece D moves at k+1 to t, and at k+2 pov captures on a square x ≠ t that D defended before k+1. "Forced" is read as "the reply to a capture or check"; we do not prove the reply was the only move.
   - `sacrifice`: some pov ply in the window has `settled(k) ≤ −2` against the starting balance, the line still gains at least 2 or mates, and the opponent does not promote in the window. `settled` comes from Task 4's `settled_plies`, whose recapture chain stops before a pov mate, so a mating sacrifice reads as −5.
   - `promotion`: a pov move in the window promotes.
   - `mate`: `mate` plus `mateIn1`, `mateIn2`, `mateIn3` or `mateIn4Plus` from `mate_in`; `matingNet` when `mate_in ≥ 2`.
   - `backRankMate`: a rook or queen gives the mate from the mated king's back rank, the king stands on that rank, and every forward escape square holds one of the king's own pieces.
   - `smotheredMate`: a knight gives the mate and every square next to the king holds one of the king's own pieces.

5. **Shared definitions** (from the Task 4 review):
   - **En prise** means "attacked, and undefended or attacked by a cheaper piece". The source is §4.4's fork rule ("the mover isn't en prise"). The attack queries are pseudo-legal and direct only (chess.js `attackers` parity): no x-rays, pins or counts.
   - **`gain` is 0 when a line has no pov plies.** TS `Math.max()` over an empty list would give −Infinity; we never emit it.

6. **Two spellings of a tag.** Internally and in TS-parity JSON (`LineAnalysis.tags`), tags use the TS camelCase keys (`hangingPiece`, `mateIn2`). The M3 contract (`ExemplarPosition.tags`) uses snake_case (`hanging_piece`, `mate_in_2`), produced only by `MotifTag::contract_tag` at the contract boundary.

## Consequences

- The rules are deliberately small and may miss motifs that cook.py's richer detectors catch (x-ray attacks, interference, attraction, and others), and the pseudo-legal attack queries can produce rare false positives (for example a "defender" that is itself pinned). Each new motif or refinement is a separate decision.
- Pinned by `backend/crates/profile-core/tests/motifs.rs`: the TS M2 Task 4 assertions, an exact tag set per fixture, a mate in 6 past the horizon, a late back-rank mate after a depth-1 material swing, a legal-recapture hanging piece, and an en-prise fork that does not count.
