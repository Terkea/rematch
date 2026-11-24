# How to do the EXP-0019 audit

The audit is 126 chess puzzles, each with four yes-or-no questions, on one page that runs in your browser with no network. Plan on about 85 minutes, plus 5 for three practice puzzles and 10 to read the definitions first. It can be split over two sittings.

## Before you start

1. Read the four definitions in section 4 of `preregistration.md` once, slowly. The page shows each question's definition under "Definition", so you can check it at any time.
2. Open `E:\code\chess-improver\data\exp19\audit.html` in Chrome, Edge or Firefox on the laptop. Use the same browser and the same profile throughout: your answers live in that browser until you download them. A private window loses them.
3. Leave `data\exp19\sealed\` alone. It holds the key, which says which puzzles we tagged. Opening it ends the blinding.
4. Don't look a position up anywhere while auditing: not on Lichess, not in an engine, not in the Rematch app. Judge it from the board and the line.

## For each puzzle

- The board shows the position after the opponent's last move (shaded), from the side to move. The line underneath is the puzzle's solution, both sides' moves; step through it with the arrow keys (or the buttons) as many times as you like. `+` is check and `#` is mate.
- Answer all four questions, each on its own: a position can show two motifs, or none. Most puzzles show none of some of the four.
- Keys: `Y` yes, `N` no, `U` unsure. Answering moves you to the next question; `↑` and `↓` (or `1` to `4`) pick a question; `←` and `→` step through the moves; `F` flips the board; `Enter` goes to the next puzzle; `Backspace` goes back one.
- Answer by the definition on the page, not by what you think Lichess or Rematch would say. If the motif is there by the definition, it is a yes even when something else is the point of the puzzle.

## When you are unsure

Use "unsure" when, after stepping through the line and reading the definition, you genuinely cannot say. Don't use it to save time: each "unsure" counts against a tag clearing and against it failing, so too many leave every tag where it is.

Don't go back to change earlier answers because a later puzzle changed your mind about a definition. The definitions are fixed; if one turns out to be unclear, note the item number and what was unclear, and it goes into `review.md` after the audit.

## Breaks and stopping

- The page shows the block (6 blocks of 21) and the minutes spent. You may stop after any whole block once you are past 90 minutes, or earlier when you are tired, and come back later in the same browser. At least 3 blocks make an audit.
- Stopping is your call, made on time and tiredness alone. Nobody looks at the answers until you have finished.

## When you are done

1. Click "Download answers.json" (the last puzzle's "Finish" does it too). Download it at the end of every sitting as a backup.
2. Save it as `E:\code\chess-improver\data\exp19\answers.json`, next to the page.
3. Tell Claude the audit is done. The next steps are mechanical: pin the file's sha256 in `inputs.sha256`, commit `answers.json` and the key into the experiment folder, then run the registered analysis once (`python analysis.py --out .`). Nothing is decided by hand.

If a second person is willing to judge blocks 1 and 2 (42 puzzles) as an optional check, do it before the key is committed: give them the page on their own browser profile, the definitions and this page, and save their file as `answers-second-rater.json`.
