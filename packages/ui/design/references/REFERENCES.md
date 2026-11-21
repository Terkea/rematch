# Design references

Six annotated references for the "Annotated scoresheet" direction (see `../../DESIGN.md`). They are internal only and never shipped.

## How these were made

Each PNG was drawn from scratch for this project: an HTML and SVG composition in our own tokens (Literata, Schibsted Grotesk, the day and night palettes), rendered to PNG with headless Chrome at 1600 × 1000. None of them is a screenshot of another product or a copy of a scan. The external sources below inspired specific qualities and are linked so a reader can study them, but their images are not embedded or committed.

All chess content is real:

- **Game:** Paul Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858 (the "Opera Game"), Philidor Defence. The moves are public domain and were checked against https://en.wikipedia.org/wiki/Opera_Game.
- **Evaluations:** Stockfish 19 at depth 20, run locally in the project's worker image on 2026-11-21, shown from White's point of view.
- **Severity:** Lichess's win-chance thresholds (`wc = 2 / (1 + exp(−0.00368208 × cp)) − 1`; a mover's loss of 0.1, 0.2 or 0.3 is an inaccuracy, a mistake or a blunder). This gives 4…Bxf3?! (+1.2 to +2.1), 6. Bc4?! (+2.0 to +1.3), 6…Nf6? (+1.3 to +3.0), 9…b5? (+3.1 to +5.7) and 10…cxb5?! (+5.6 to +8.1). Stockfish preferred 6…Qf6 and 10. Nxb5.
- **Pieces:** Cburnett (see "Piece sets" below), recoloured to the DESIGN.md piece tokens. They were rendered from the SVGs on Wikimedia Commons and are not committed.

## 1. Move-list density and the eval graph

File: `01-analysis-density-and-eval-graph.png`

Inspired by: the Lichess game analysis board, https://lichess.org/analysis (any analysed game). Lichess is free software under AGPL-3.0, and its page design is described here, not copied.

- **Take:** two moves per row at a 26px rhythm, so a whole short game fits beside the board. The eval graph sits full width directly under the board, with a cursor that travels with the board. The y-axis is win chance.
- **Our changes:** mistakes get the same hand-drawn ring as the board, thinner for inaccuracies. Labels sit in a lane above the plot with leader ticks, never on the line. The eval bar beside the board uses the same win-chance scale.
- **Don't copy:** the board colours, the dark tool chrome, the tabbed engine panel, or engine variations as the main content.

## 2. Mark semantics and the hand-drawn pen

File: `02-arrow-semantics-and-pen-stroke.png`

Inspired by: Lichess studies with drawn shapes, https://lichess.org/study (colour carries meaning).

- **Take:** colour equals meaning. In our system, red pen is the mistake, green is the better move or the punishment, and pencil is a plan.
- **Don't copy:** uniform, translucent, constant-width strokes. The top-left pair shows the difference at thumbnail size: a mechanical circle and arrow next to ours, which is a tilted, tapered spiral with an unequal two-stroke head on a sheet halo.
- **Also shown:** the severity scale (blunder, mistake, inaccuracy, good move, brilliant) with the glyph-badge and head-shape redundancy; the marks in day and night; and the draw sequence at 200, 360, 520 and 720ms.

## 3. The ruled scoresheet and red corrections

File: `03-scoresheet-grid-and-corrections.png`

Inspired by: photographs of handwritten tournament scoresheets. One example on Wikimedia Commons is Magnus Carlsen's scoresheet from game 9 of the 2018 World Championship, https://commons.wikimedia.org/wiki/File:Planilla_Magnus_Carlsen.jpg, by Pablo Martínez Rodríguez (Cheposian), **CC BY-SA 4.0**. Because that licence is share-alike, the photo is linked only and nothing from it is reproduced.

- **Take:** the ruled grid, the ink weight (moves in Literata roman 600, heavier than the rules), and the coach's marks: the scoresheet ring on mistakes, glyphs, and whole-sentence italic margin comments.
- **Don't copy:** paper grain, carbon-copy tint, federation header boxes, a handwriting font, or italic moves.

## 4. Notation in running text and the captioned diagram

File: `04-book-notation-and-diagram-caption.png`

Inspired by: J. Löwenthal (ed.), *Morphy's Games: a selection of the best games played by the distinguished champion in Europe and America* (London, 1860), https://archive.org/details/morphysgamesase00lwgoog. It is public domain by age (Löwenthal died in 1876), and it is still described only.

- **Take:** notation set inline in the text face, a heavier main line, a short italic caption under the diagram, and a book-page measure.
- **Don't copy:** descriptive notation, the long s, fleurons, justified text with rivers, or foxing. The commentary in the PNG is ours, written from the engine data.

## 5. Night study

File: `05-night-study-analysis.png`

Reference 1 recomposed in the night theme, to check that night keeps the "desk lamp" character: navy paper, a lighter navy sheet, near-white ink, the warmer night pen, the halo on every mark, `well`-outlined pieces and coordinates that switch colour by square. No external source.

## 6. Piece sets

File: `06-piece-set-candidates.png`

Three candidates whose licences allow shipping in a closed-source commercial web app, shown on our squares in day and night. GPL, AGPL, non-commercial (NC) and "freeware" sets were excluded first. Licences were read from the sources below on 2026-11-21.

| Set | Author | Licence | Source | Notes |
|---|---|---|---|---|
| **Cburnett (recommended)** | Colin M.L. Burnett | Multi-licensed on Commons: GFDL 1.2+, CC BY-SA 3.0, **BSD-3-Clause** or GPLv2+, at the user's choice. We take BSD-3-Clause. | https://commons.wikimedia.org/wiki/File:Chess_klt45.svg and its siblings (`Chess_{k,q,r,b,n,p}{l,d}t45.svg`) | Flat outlines, 1.5px stroke at 45px. It recolours with two values and has the most familiar silhouettes. The BSD notice and attribution go on `/licenses`. Take the files from Commons, not from Lichess's repo, where Lichess lists its copy as GPLv2+. |
| Chessnut | Alexis Luengas | Apache-2.0 (`LICENSE.txt` in the repo) | https://github.com/LexLuengas/chessnut-pieces (also in `lichess-org/lila`, `public/piece/chessnut`) | Clean and modern, with heavier fills, and it recolours well. Rounder and more toy-like, and its fills dominate our low-contrast squares. The NOTICE and licence go on `/licenses`. |
| RhosGFX | RhosGFX | CC0 1.0 | `lichess-org/lila`, `public/piece/rhosgfx` (licence listed in lila's `COPYING.md`) | The freest licence, but a multi-colour illustration set in warm orange and clay. It collides with `B1` and would need a full redraw to fit the palette. |

The decision record is `docs/decisions/ADR-WEB-n-piece-set.md`. The owner makes the final taste call.

## Regenerating

The generator script was a one-off and is not committed. To redo a reference, recompose it the same way: HTML and SVG using only DESIGN.md tokens, the stroke geometry from DESIGN.md, real game data and the chosen piece set, rendered with Playwright or headless Chrome at 1600 × 1000.
