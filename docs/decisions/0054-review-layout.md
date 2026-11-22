# 0054. Game review layout: the graph as the spine, with the mistakes ledger

Date: 2026-11-22. Status: accepted (controller ruling after the Task 7 critic's ranking).

## Context

The single-game review page (M5a Task 10) shows one game with the board, eval bar, eval graph, scoresheet with each move's judgement, best-move arrows and clocks. Task 7 built three layouts in the dev-only lab (`/lab/review/{a,b,c}`, behind `labEnabled` in the layout and the loader) and ran them on the owner's real analysed games, copied read-only into a throwaway compose project's database (the labs-on-real-data ruling). The variants differ on what leads:

- **a, "Analysis desk":** the board leads; eval bar left, graph under the board, a tall scoresheet column on the right.
- **b, "Graph as the spine":** a full-width graph on top drives the board; the player's mistakes are a strip of jumps; the scoresheet is a drawer on phones.
- **c, "Annotated sheet":** the sheet leads: the shown move set large with its commentary line, a "Where it went wrong" ledger with each mistake's win-chance drop, the ruled moves, and the graph as the footer; the board floats beside it.

## Decision

- A fresh critic, judging the screenshots only, ranked **b > c > a**: b is the only layout built around the job (the whole game's shape at a glance, the verdict sentence loudest); c has the most useful element (the ledger) in the wrong layout; a is generic.
- **Ruling: variant b, with c's ledger replacing b's chip grid.** The implementer's earlier pick of c is superseded. The critic's changes are Task 10 hand-offs, listed in `packages/ui/design/decisions/review.md` and `task-10-brief.md`.
- **Client boundary.** The review page hands its client component a narrow view model with only what it renders (names and ratings, the meta and result lines, accuracies, the per-ply SAN, uci, FEN after, clock and the analysis fields it draws), never the raw `ReviewGame`: no id, platform, source URL, ECO or opening name unless the page shows them. The lab's `view.ts` is the pattern, with a test that the client props carry only allowed keys.
- **Lab loader.** `loadLabReview(gameId?)` gives the given game, or the user's analysed game with the most moments (ties to the latest `played_at`), trying candidates in that order until one passes ADR 0049's analysis guard. It refuses in production before any auth or read, as the layout does.
- **Real data.** Every one of the owner's analysed games in the lab copy passed ADR 0049's strict guard, so none is shown as unanalysed. The copy carries no `position_evals`, so every score there is derived and shown with "≈". The counts are in the git-ignored task report.

## Consequences

The review answers "where did I go wrong" with the whole game's shape first and the verdict in words beside the board, and the ledger replaces a wall of red buttons. Task 10 carries the critic's fixes (graph labels and scale, the phone's thumb bar and graph height, the board mark, severity-graded glyphs, an honest judgement for every printed number). Up to 25 sequential `getGameReview` calls in the lab's default pick are fine for a lab; the product page loads one game by id.
