# 0051. Game review interaction, board marks and honest judgements

Date: 2026-11-22. Status: accepted (M5a Task 10).

## Context

`/app/games/[gameId]` is where a user steps through one analysed game. Task 7 chose the layout (ADR 0054: variant b, the win-chance graph as the spine, with c's "Where it went wrong" ledger), and its critic listed the changes Task 10 must make: severity-graded marks, graph fixes, a phone layout for one hand, a board mark that doesn't cover the piece, and a judgement that never contradicts the number printed beside it. The plan's rulings R8 and R9 and Task 5's key map are recorded here.

## Decision

**State.** A pure reducer (`review-state.ts`) over `{ ply, playing, orientation, showEngine }`. The ply is clamped to `0..plies`. Opening ply: an integer `?ply=` (clamped), otherwise the player's worst moment, otherwise the last ply. The board faces the player's colour. Any go, step, first, last or moment stops play; autoplay ticks every 1000 ms and stops at the end, and play pressed at the end restarts from move 1. The ply is mirrored to `?ply=` with `history.replaceState`, 150 ms after the last step, and never on the first render.

**Announcements.** One page-level polite `role="status"`, present at every width, reads the shown move's verdict (or the move) 300 ms after the last step. It is silent on first paint and while autoplaying, and reads the move play stops on. The scoresheet's own announcer is off, so a step is read once, and phones (whose scoresheet is in a closed drawer) are not left silent (fix round 1).

**Keys** (Task 5's `useReviewKeys`, with its widened guard): ArrowLeft and ArrowRight step, Home and End jump, ArrowUp and ArrowDown go to the previous or next mistake, `f` flips, Space plays, `b` toggles the engine move. The graph's slider, the scoresheet and dialogs own their keys, so a key moves one ply, never two. The scoresheet is one tab stop (the step announcement is page-level, above); autoplay is the fixed "Autoplay" button with `aria-pressed`; touch targets are 44 px.

**Sound hook (R9).** `GameReviewView` calls `onStep(kind, ply)` with `step` (keys and buttons), `play` (ticks) or `jump` (scoresheet, ledger, first, last, mistakes, and a graph scrub once it has settled for 150 ms, never on every scrub frame). M4 T12b wires sounds to it.

**Board marks (R8, revised).** At the shown ply:
- The player's mistake or blunder: a red-pen arrow of the move played, from the square it left to the *edge* of the square it reached (`toEdge`, the ringed-target clearance), so no ink covers the piece.
- No ring. On a phone the ring, the arrowhead and the badge made one red blob over the piece (critic, T7 change 4). This replaces the plan's "pen ring on the destination".
- Every judged move: the glyph badge centred on a top corner of the destination (`place: "edge"`, half outside the square, kept on the board), so it never sits on the piece. It takes the top-left corner when an arrow arrives at the top right.
- Every judged move where the engine preferred another move: that move as a `good` arrow with a solid head. A line under the verdict names it ("Green arrow: Bxe6, the engine's choice instead."), so the arrow never looks like a switch that failed to turn off.
- "Show the engine's next move" (off by default, `b`): one pencil arrow, the engine's choice in the shown position. It was called "Show engine move", which read as broken beside an always-on green arrow (fix round 1).

**Severity.** Only the player's `??` is in full pen. `?` is a lighter pen (75% on text, 60% on graph ticks, and on the board a pen outline on paper, `weight: "light"`) and `?!` is ink or pencil, in the scoresheet, the ledger, the graph's ticks and labels and the board's badges. The opponent's marks are graphite, whatever their severity. "Best" is tagged on the player's own moves only. The collapsed ledger ranks `??` before `?` before `?!`, then by drop, so a forced-mate blunder with a small drop is never hidden.

**Graph.** White is always at the top, and the panel says so. There are rules at 25% and 75% and a named 50% line. The corner names sit on a solid ground so the line never runs through them. There are no bare-glyph labels (a label that doesn't fit is dropped; the text alternative still lists it). Above 100 plies, or when the serious marks crowd the lane, only blunders are labelled and mistakes stay ticks. A phase name that would run into the next one is left off. The graph is the scrubber, with a cursor at the shown ply. On a phone it is a 70 px strip (`compact`: plot only), so the verdict clears the first screen.

**Phone.** The eval bar names both ends ("White", "Black"), and the number is in the readout. A sticky bar under the thumb holds the previous mistake, previous, Autoplay, next and the next mistake (labelled "Mistake", with double carets, so it doesn't read as collapse and expand). The scoresheet is a drawer.

**Honest judgements.** The stored rule (`judge`, packages/chess-core `judgement.ts`, ported unchanged to Rust) is Lichess's:
- A drop in the mover's win% of 15, 10 or 5 points is a blunder, a mistake or an inaccuracy.
- Losing a forced mate is a blunder, unless the move still leaves +7 or more (then a mistake at +7 to +10, an inaccuracy above +10).
- Walking into a forced mate is a blunder, unless the position was already −7 or worse.

A mate maps to the cp clamp (win% 97.54 or 2.46), so a lost mate can print as "98% to 90%" and still be a blunder, while an 11-point drop is a mistake. The judgement is not wrong. What it was judged on was missing.

The view model therefore derives each judged move's basis from the stored numbers (`judgementBasis`):
- `win` when the drop explains the judgement;
- otherwise `missedMate` (the move started at the mate clamp) or `allowedMate` (it ended at the clamp).

The page then prints that basis:
- the verdict reads "… is a blunder: it let a forced mate go. Win chance 98% to 90%." (or "it allows a forced mate");
- the ledger row adds "marked for losing a forced mate, not for the drop".

On the owner's data, every judgement more severe than its drop was a mate rule at the clamp, and none was less severe. No judgement is reclassified.

**Client boundary.** The server component renders the header (title, meta, result, accuracies, the source link) and hands `GameReviewView` only `{ view, initialPly }`.
- The view carries the names and ratings, colour, start FEN and score, status, clocks, phases, moments, and per ply the SAN, uci, FEN after, clock and the analysis fields drawn.
- The win% before and after is included only on judged plies, where it is printed.
- It never carries the id, platform, source URL, opening, result, termination or accuracy.

**Not analysed.** The board, scoresheet, clocks and controls still work, and there is no eval bar or ledger.
- A waiting game: a graph placeholder ("Analysing") and "This game is waiting for analysis. You can replay the moves now."
- A failed game: "Analysis failed for this game."
- A skipped game: "This game was skipped: it is a variant or too short to analyse."

Not found (another user's game, an unknown or a malformed id, all alike): "We couldn't find that game." with a link back to Games.

## Consequences

The mark beside a number is always explained by that number or by the words next to it. The board mark leaves the piece readable at 390 px. `EvalGraph` gains `compact`, `EvalBar` gains `ends`, `ReviewControls` gains `layout="thumb"`, and the `arrow` annotation gains `toEdge`. All of them are opt-in, and only the graph's label rules (no bare glyphs, sparse long games), its scale and severity grading, and the scoresheet's severity grading change every host.

Left for later:
- the dark-mode board contrast (black pieces on dark squares);
- the vertical eval bar's tiny rotated number on desktop (the readout carries the number);
- the critic's "≈ or round to 5%" on win chances (the verdict prints whole percents, as the plan specifies).
