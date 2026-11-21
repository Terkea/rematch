# ADR-WEB-n. Piece set

Date: 2026-11-21
Status: Proposed (placeholder; the owner makes the final taste call, and the controller assigns the number)

## Context

The board needs a piece set that fits the "Annotated scoresheet" direction (`packages/ui/DESIGN.md`, "Pieces"): flat, in the ink metaphor, recoloured to `--color-piece-dark` and `--color-piece-light`, with a dark outline that holds 3:1 on both squares in both themes. The web client is closed source and commercial, so the licence must allow that. GPL and AGPL image sets would put the client's licensing in question, and non-commercial (NC) and "freeware" sets are excluded outright. A CC BY-SA set would be acceptable only as images released under CC BY-SA with attribution on `/licenses`.

## Candidates

The licences were read on 2026-11-21. Details are in `packages/ui/design/references/REFERENCES.md`, and the side-by-side comparison is in `packages/ui/design/references/06-piece-set-candidates.png`.

| Set | Licence | Source |
|---|---|---|
| Cburnett (Colin M.L. Burnett) | Multi-licensed: GFDL 1.2+, CC BY-SA 3.0, BSD-3-Clause or GPLv2+. We choose BSD-3-Clause. | Wikimedia Commons, `Chess_{k,q,r,b,n,p}{l,d}t45.svg` |
| Chessnut (Alexis Luengas) | Apache-2.0 | github.com/LexLuengas/chessnut-pieces |
| RhosGFX | CC0 1.0 | lichess-org/lila, `public/piece/rhosgfx` |

## Decision (proposed)

Use **Cburnett under its BSD-3-Clause option**, taken from Wikimedia Commons, recoloured at build time to the piece tokens. Ship the BSD notice and attribution on `/licenses`.

## Consequences

- The pieces are modified (recoloured), which BSD-3-Clause permits as long as the notice is kept.
- The most familiar silhouettes in online chess help players read positions fast. The trade-off is that the set is familiar rather than unique, and the board's identity comes from the squares, the marks and the motion.
- If the owner prefers Chessnut, the Apache-2.0 NOTICE and licence go on `/licenses` instead. RhosGFX would need a redraw to fit the palette (ban `B1`).
