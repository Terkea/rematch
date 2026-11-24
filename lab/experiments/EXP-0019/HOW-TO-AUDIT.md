# How to do the EXP-0019 audit

The audit is 132 chess puzzles in 6 blocks of 22, each puzzle with four yes-or-no questions, on one page that runs in your browser with no network. Plan on about 90 minutes of judging, plus 5 for three practice puzzles and 10 to read the definitions first. Two sittings of three blocks each work as well as one.

## Before you start

1. Read the four definitions in section 4 of `preregistration.md` once, slowly. The page shows each question's definition under "Definition", so you can check it at any time.
2. Open `E:\code\chess-improver\data\exp19\audit.html` in Chrome, Edge or Firefox on the laptop. Use the same browser and the same profile throughout: your answers live in that browser until you download them. A private window loses them.
3. Leave `data\exp19\sealed\` alone. It holds the key, which says which puzzles we tagged. Opening it ends the blinding.
4. Don't look a position up anywhere while auditing: not on Lichess, not in an engine, not in the Rematch app. Judge it from the board and the line.

## For each puzzle

- The board shows the position after the opponent's last move (shaded), from the side to move. The line underneath is the puzzle's solution, both sides' moves; step through it with the arrow keys (or the buttons) as many times as you like. `+` is check and `#` is mate.
- Answer all four questions, each on its own. A puzzle can show one of the four motifs, several, or none.
- Answer yes when the definition holds and no when it does not, whatever the puzzle seems to be about.
- Keys: `Y` yes, `N` no, `U` unsure. Answering moves you to the next question; `↑` and `↓` (or `1` to `4`) pick a question; `←` and `→` step through the moves; `F` flips the board; `Enter` goes to the next puzzle; `Backspace` goes back one.

## When you are unsure

Use "unsure" only when, after stepping through the line and reading the definition, you genuinely cannot say.

Don't go back to change earlier answers because a later puzzle changed your mind about a definition. The definitions are fixed; if one turns out to be unclear, note the puzzle number and what was unclear, and it goes into `review.md` after the audit. The page logs every answer with its time.

## Breaks and stopping

- The audit is all 6 blocks. Breaks go between blocks: the page says when a block is done and shows the minutes spent. Come back later in the same browser and it carries on where you were.
- If you have to stop for good before block 6, write down why before you would have opened the next block, and put that note next to `answers.json`. Only whole blocks are analysed, and at least 3 make an audit.
- Nobody looks at the answers until you have finished.

## When you are done

1. Click "Download answers.json" (the last puzzle's "Finish" does it too). Download it before every break as a backup; the file that counts is the one downloaded at the end of the last sitting.
2. Save it as `E:\code\chess-improver\data\exp19\answers.json`, next to the page.
3. Tell Claude the audit is done. The next steps are mechanical: pin the file's sha256 in `inputs.sha256`, commit `answers.json` and the key into the experiment folder, then run the registered analysis once (`python analysis.py --out .`). Nothing is decided by hand.

If a second person is willing to judge blocks 1 and 2 (44 puzzles) as an optional check, do it before the key is committed: give them the page on their own browser profile, the definitions and this page, and save their file as `answers-second-rater.json`.
