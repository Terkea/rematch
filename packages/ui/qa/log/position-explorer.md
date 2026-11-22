# Position explorer (`@ci/ui/position-explorer`): critique log (feat/position-explorer)

Screenshots are not committed. They were taken in the lane's e2e stack (`ci-fixf`, qa container,
reduced motion) of `/styleguide#position-explorer`: the compact boards, then the dialog on 28. Nd2??
(the game, after one step, and after trying 28. Nc3) and on 1. d4 (the engine line), at 390 and 1440,
day and night (`.shots/explorer/*.png`), plus `pnpm shots --routes /styleguide`. Chess is the
fixture game DrNykterstein v Night-King96; the fixture stores no engine line for 28. Nd2, so that
demo shows the honest "No engine line is stored for this move."

No horizontal scroll on the page or inside the dialog at 390 or 1440. No `NaN|undefined|Infinity|null`
in the dialog (unit test).

## Pass 1 (implementer self-critique; self-critique only)

The lane was told not to dispatch subagents, so the fresh critic pass is still owed: **the
controller runs it and appends the verdict here.** `pnpm design:check`: 0 findings.

Found and fixed:

1. **The three-way line control was clipped** ("Try a m…") in the 304px side column at 1440 and at
   390 (it scrolled inside its track). "What you played" and "Engine line" stay a segmented
   control; "Try a move" is its own toggle button (`aria-pressed`), which a move on the board also
   presses. That is also closer to the job: the two lines are views of stored chess, trying is an
   action.
2. **"Your own line."** said nothing. A tried move now reads "You tried 28. Nc3." (or that it is
   the engine's move, or the game's).
3. **The notes grew** when "Try a move" added its engine sentence, pushing the move list down.
   The note block keeps a minimum height that holds the longest note at 390px (`min-h-28`,
   `lg:min-h-20`), so switching lines moves nothing below it.
4. **Focus fell to the page** when Next or Last disabled itself at the end of the line; it now
   moves to the first enabled step button (as `ReviewControls` does).

Checked and kept:

- Tokens only; Phosphor icons through `Icon` (`ArrowsOutSimpleIcon` added to the curated set).
- The board keeps the halo rule and the tones: the played move in pen for the player's own mistake
  (pencil otherwise) and the better move in good, both only on the cited position.
- One primary fill at most (the pressed "Try a move"); the dialog is the shared `DialogContent`
  (sheet, `--radius-sheet`, the one lift shadow, night edge).
- Honesty: tried moves are never evaluated ("Moves you try are not checked by the engine").
- Reduced motion: the dialog fades only, pieces jump, the segmented chip and the scoresheet cursor
  move without springs.

Open (minor): on a phone the move list sits below the fold of the dialog, which scrolls; the board
and the step controls come first, which is the order of use.
