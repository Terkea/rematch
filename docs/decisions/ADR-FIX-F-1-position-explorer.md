# ADR-FIX-F-1. One position explorer for every cited position

Date: 2026-11-22. Status: accepted (owner: positions should be "extendable and interactible").
Builds on ADR-FIX-D-1 (answers show their positions), ADR-WEB-13 (board) and ADR 0062
(interactive engine checks).

## Context

Fix D gave coach answers small static boards: the position before a cited move, a "Played" and a
"Better" arrow and a link to the review. The owner wants them to open up: step through the game,
see the engine's line, try moves. Weakness examples and the Strengths page (other lanes) will
cite positions too, so this must be one shared component.

## Decision

1. **`PositionExplorer` in `@ci/ui/position-explorer`.** Props: `gameId`, `ply`, `fenBefore`,
   `played`, `best`, `orientation`, `label` (notation), `mistake`, `href`, and `loadLine` or
   `line`. Compact by default (the Fix D board, its key, an Expand button and the game link; a
   click on the board opens it too). Expanded, it is a modal dialog (Base UI: focus trapped,
   Escape closes, focus returns to Expand), sized to the viewport with a whole-pixel board.
2. **Three lines through one position.** "What you played" (the game, eight plies either side of
   the cited one), "Engine line" (the game up to the cited ply, then the stored engine line) and
   "Try a move" (the shown line up to where the player started, then their own legal moves,
   played with chess.js through `Board`; Reset goes back). A move on the board switches to trying
   by itself. The board starts on the position before the cited move, from the player's side,
   with a flip. Arrow keys, Home, End and F step and flip; the move list is the review's
   `Scoresheet`, which announces the move landed on.
3. **Data.** The compact state comes with the page, batched (`citedPositions` with the thread,
   now with `reviewGameId`). The expanded data is fetched once, on first expand, from
   `GET /api/games/<games.id>/line?ply=N` (`server/games/position-line.ts`): the game's moves
   from the PGN, as the review reads them, and `move_features.best_pv` from the position before
   the move, else the analysis' `best_uci` alone, cut at the first illegal move, at most 16 plies.
   Until it answers, the dialog already has the move played and the engine's move.
4. **Tried moves get no evaluation.** The only interactive engine path (ADR 0062) runs inside a
   coach turn, through the worker's quota, and the web has no route to it. Inventing an eval, or
   a web path that bypasses those quotas, is not acceptable, so a tried move says "Moves you try
   are not checked by the engine" and points to the review. It does say when the tried move is
   the engine's move or the game's, which is stored fact.
5. The review page keeps its own board: it is already the full explorer of the game.

## Consequences

- Other surfaces adopt it with the same props and `loadLine={fetchPositionLine}`
  (`apps/web/src/lib/position-line.ts`).
- A web-triggered engine check for tried moves needs its own decision (a route that reserves an
  `interactive_evals` row against the plan's quota and enqueues `interactive-analyse`).
