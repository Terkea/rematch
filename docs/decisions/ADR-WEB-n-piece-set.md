# ADR-WEB-n. Piece set

Date: 2026-11-21
Status: Accepted (owner decision, 2026-11-21; the controller assigns the number)

## Context

The board needs a piece set that fits the "Annotated scoresheet" direction (`packages/ui/DESIGN.md`, "Pieces"): flat, in the ink metaphor, recoloured to `--color-piece-dark` and `--color-piece-light`, with a dark outline that holds 3:1 on both squares in both themes. The web client is closed source and commercial, so the licence must allow that. GPL and AGPL image sets would put the client's licensing in question, and non-commercial (NC) and "freeware" sets are excluded outright. A CC BY-SA set would be acceptable only as images released under CC BY-SA with attribution on `/licenses`.

## Candidates

The licences were read on 2026-11-21. Details are in `packages/ui/design/references/REFERENCES.md`, and the side-by-side comparison is in `packages/ui/design/references/06-piece-set-candidates.png`.

| Set | Licence | Source |
|---|---|---|
| Cburnett (Colin M.L. Burnett) | Multi-licensed: GFDL 1.2+, CC BY-SA 3.0, BSD-3-Clause or GPLv2+. We choose BSD-3-Clause. | Wikimedia Commons, `Chess_{k,q,r,b,n,p}{l,d}t45.svg` |
| Chessnut (Alexis Luengas) | Apache-2.0 | github.com/LexLuengas/chessnut-pieces |
| RhosGFX | CC0 1.0 | lichess-org/lila, `public/piece/rhosgfx` |

## Decision

Piece sets are **user-configurable** ("Pieces" in Settings, under Board, stored per user and applied to every board).

- **Default: Cburnett, under its BSD-3-Clause option.** It is taken from Wikimedia Commons, not from Lichess's GPL-labelled copy, and recoloured at build time to `--color-piece-dark` and `--color-piece-light`. The BSD notice and attribution go on `/licenses`.
- **Option: Chessnut, under Apache-2.0.** It is taken from github.com/LexLuengas/chessnut-pieces and recoloured the same way. The Apache-2.0 licence and attribution go on `/licenses`.
- **Not offered: RhosGFX.** Its multi-colour, warm palette breaks the recolouring rules (ban `B1`).
- **Reserved:** a slot for a future commissioned signature set, which needs a written licence or assignment to us.
- **Licence gate for any future set:** no GPL, AGPL, NC, freeware or unlicensed sets. The licence, author and exact source are recorded in REFERENCES.md and here before the set is added, and attribution goes on `/licenses`. Every set must pass the recolouring rules in DESIGN.md, "Piece sets".

## Consequences

- The pieces are modified (recoloured), which BSD-3-Clause permits as long as the notice is kept.
- The most familiar silhouettes in online chess help players read positions fast. The trade-off is that the set is familiar rather than unique, and the board's identity comes from the squares, the marks and the motion.
- Shipping two sets means two build-time sprite sets per theme. That cost is small, but it is a cost.
- Both sets' licence texts must be on `/licenses` from the first release that offers them.
