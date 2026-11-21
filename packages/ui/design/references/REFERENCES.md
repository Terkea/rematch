# Design references

Four annotated references for the "Annotated scoresheet" direction (see `../../DESIGN.md`). They are internal only and never shipped.

## How these were made

Each PNG was drawn from scratch for this project: an HTML and SVG composition in our own tokens (Literata, Schibsted Grotesk, the day palette), rendered to PNG with headless Chrome at 1600 × 1000. None of them is a screenshot of another product or a copy of a scan. The external sources below inspired specific qualities and are linked so a reader can study them, but their images are not embedded or committed.

All chess content is real:

- Game: Paul Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858 (the "Opera Game"), Philidor Defence. The moves are public domain.
- Evaluations: Stockfish 19 at depth 20, run locally in the project's worker image on 2026-11-21, shown from White's point of view.
- Marked moves follow the engine's swings: 6…Nf6 (+1.3 to +3.0, Stockfish preferred 6…Qf6), 9…b5 (+3.1 to +5.7), 10…cxb5 (+5.6 to +8.1).
- The pieces are Unicode chess glyphs (Segoe UI Symbol), used only as a placeholder. The production piece set is chosen in a later task.

## 1. Move-list density and the eval graph

File: `01-analysis-density-and-eval-graph.png`

Inspired by: the Lichess game analysis board, https://lichess.org/analysis (any analysed game, for example from https://lichess.org/games). Lichess is free software under AGPL-3.0, and its page design is described here, not copied.

- **Take:** two moves per row at a tight rhythm, so a whole short game fits beside the board. The eval graph sits full width directly under the board, with the current move marked by a cursor line that travels with the board. Mistakes are marked on the line itself.
- **Don't copy:** the board colours (brown, green or blue squares), the dark tool chrome, the tabbed engine panel, or engine variations as the main content. We show one coach mark per mistake, and variations stay one click away.

## 2. Arrow semantics and the hand-drawn stroke

File: `02-arrow-semantics-and-pen-stroke.png`

Inspired by: Lichess studies with drawn shapes, https://lichess.org/study (shapes are drawn with right-click and modifier keys, and the colour carries meaning).

- **Take:** colour equals meaning. In our system, red pen is the mistake, green is the better move or the punishment, and pencil is a plan or a neutral idea. Circles mark squares, and arrows mark moves.
- **Don't copy:** the uniform, translucent, mechanical strokes. Ours are opaque, drawn once like a pen, and sit on a 2px `--color-sheet` halo. The circle starts near 1 o'clock and overruns to 380°. The arrowhead is two quick strokes that start after 85% of the shaft. The bottom strip shows the draw sequence frame by frame, and the night board shows the halo doing its job on light night squares.

## 3. The ruled scoresheet and red corrections

File: `03-scoresheet-grid-and-corrections.png`

Inspired by: photographs of handwritten tournament scoresheets. One example on Wikimedia Commons is Magnus Carlsen's scoresheet from game 9 of the 2018 World Championship, https://commons.wikimedia.org/wiki/File:Planilla_Magnus_Carlsen.jpg, by Pablo Martínez Rodríguez (Cheposian), **CC BY-SA 4.0**. Because that licence is share-alike and not public domain, the photo is linked only and nothing from it is reproduced. Our composition uses the Opera Game instead.

- **Take:** the ruled grid, with one hairline per move pair and the number column set off by a vertical rule. The ink weight, where moves sit a little heavier than the rules. The coach's red marks: a loose ring, a question mark, and a short italic margin note.
- **Don't copy:** paper grain, carbon-copy tint, federation header boxes, or a handwriting font. Ruling is allowed only on scoresheet and move-list surfaces (see DESIGN.md, ban `B3`).

## 4. Notation in running text and the captioned diagram

File: `04-book-notation-and-diagram-caption.png`

Inspired by: 19th-century annotated game collections, in particular J. Löwenthal (ed.), *Morphy's Games: a selection of the best games played by the distinguished champion in Europe and America* (London, 1860), https://archive.org/details/morphysgamesase00lwgoog. It is public domain by age (Löwenthal died in 1876), but it is still described only, not reproduced.

- **Take:** notation set in the text face, inline in the sentence, with figures that align. The main line is set heavier than the comment. A short italic caption under the diagram names the moment. Prose has a book-page measure.
- **Don't copy:** descriptive notation (P-K4), the long s, fleurons and ornamental rules, justified text with rivers, or foxing. The century gives us rhythm, not costume. The commentary text in the PNG is ours, written from the engine data above.

## Regenerating

The generator script was a one-off and is not committed. To redo a reference, recompose it in the same way: HTML and SVG using only `DESIGN.md` tokens and real game data, rendered with Playwright or headless Chrome at 1600 × 1000.
