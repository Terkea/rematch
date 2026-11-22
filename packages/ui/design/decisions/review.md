# Game review layout

Date: 2026-11-22. Task 7 (M5a). Status: ruled. The controller ruled variant b with c's "Where it went wrong" ledger, after a fresh critic ranked the variants b > c > a (LP step 3). ADR 0054 records the same.

Lab routes: `/lab/review/a`, `/lab/review/b` and `/lab/review/c`, each with `?game=<id>` (default: the signed-in user's analysed game with the most moments, the latest on a tie). The variants ran on the owner's real analysed games in a throwaway lab database, never on fixtures: a won game, a lost game with many blunders, and the longest game in the set (a long draw). No owner game, id, name, move or statistic is committed. The screenshots and the real numbers stay in the git-ignored ledger, `.superpowers/sdd/2026-11-21-05a-game-pages/` (`shots-t7/review-{a,b,c}-{win,loss,long}-{1440,390}-{light,dark}.png`, the 390 `-fold.png` first screens, `review-b-loss-390-light-drawer.png`, the critic's `critique-t7.md` and `task-7-report.md`).

Every variant uses the same parts: `Board` with the R8 marks, `EvalBar`, `EvalGraph`, `Scoresheet`, `ClockPair`, `ReviewControls` and the review keys, with a `≈` on every derived eval. At 390px each keeps the board, the eval bar and the controls in the first viewport. The client views get a narrow view model (`apps/web/src/app/lab/review/view.ts`), never the raw `ReviewGame`.

## The variants

| | a, "Analysis desk" | b, "Graph as the spine" | c, "Annotated sheet" |
|---|---|---|---|
| Lead | the board | the eval graph, full width on top | the sheet: the shown move and its coach's line |
| Scoresheet | a tall right column, as tall as board, controls and graph | beside the board from 768px; a drawer on phones | inside the sheet, beside "Where it went wrong" |
| Graph | under the board | full width, first | the sheet's footer |
| Commentary line | none (move and eval only) | under the shown move, beside the board | set large in the sheet, in Literata italic |
| Player's mistakes | the graph's labels only | a strip of jump buttons | a ledger: move, win chance before and after |

## Critic's ranking (fresh subagent, screenshots only)

1. **b, best.** The only variant built around the job: the full-width graph shows the whole game at once (in the long game it is the only layout where its shape reads), and the verdict sentence is the loudest text on the page. Weak points: the chip grid is generic and noisy, and the graph is cramped on the phone.
2. **c, the best idea in the wrong layout.** The italic commentary and the "Where it went wrong" ledger with win-chance deltas are the most useful and most honest elements in the set, but the board sits on the right with the clocks orphaned under it, and the graph is at the bottom (last on the phone).
3. **a, the most generic.** A standard board, move list and mini-graph with no explanatory sentence; on the phone the verdict is off the first screen.

## Ruling: variant b, with c's ledger

The implementer's earlier pick, c, is **superseded** by the critic's ranking and the controller's ruling.

### Taken from the losers

- From c: the "Where it went wrong" ledger (move, win chance before and after) replaces b's chip grid; the italic serif commentary; the ruled card that groups the verdict, moves and ledger.
- From b's drawer: the move list as a bottom sheet on phones, and the opponent-aware wording ("… is a blunder by <opponent>. Your win chance a% to b%.").
- From a: the clock block with the time spent on the move.

### Hand-offs to Task 10 (already in `task-10-brief.md`; not built in the lab)

1. The ledger replaces the chips, ruled like a scoresheet, with the current row highlighted and the glyphs graded by severity (only `??` in full red pen); collapse long ledgers.
2. Graph legibility: no bare `?` labels, no overlapping phase labels, a sparse label set on long games, a scale, one fixed side-up rule (labelled), and scrubbing with a current-move cursor.
3. Phone: a graph strip of about 70px so the verdict clears the fold, a sticky bottom thumb bar (previous and next, previous and next mistake, labelled as such), and both ends of the eval bar labelled.
4. Redesign the red-pen board mark so the badge, ring and played-move arrow never cover the piece, and keep the `good` arrow clearly different; check the night board's contrast for dark pieces on dark squares.
5. The move table: clip rows under the sticky header, show "Best" pills on the player's moves only, and never show a judgement that contradicts the number beside it (investigate the stored judgement rule; prefix win chances with `≈` or round them).
6. Client boundary: the page passes a narrow view model, as the lab now does.
