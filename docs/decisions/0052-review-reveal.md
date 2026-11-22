# 0052. The analysis reveal on the review page

Date: 2026-11-22. Status: accepted (M5a Task 11).

## Context

DESIGN.md Choreography §1 pins "the performance": on the first view of an analysed game, a prologue draws the win-chance graph, then the player's three worst moments are *played* on the board in ascending severity, ending on the worst. M7 makes it interruptible, M8 runs it in full once and condensed afterwards, and M10 replaces it with its end state under reduced motion. The plan's ruling R10 says where "seen" lives and which moments play.

## Decision

**R10, seen per device.** DESIGN.md M8 asks for "seen" on the server, per user and game. M5a stores it per device instead: `localStorage["ci.review.seen"]`, a JSON array of at most 500 game ids, oldest dropped; re-adding an id moves it to the newest end (`reveal/seen-store.ts`). Every access is in try/catch, and a store that throws reads as **seen**, as does one that reads but cannot write (`add` reports failure, and the run is condensed; fix round 1), so a blocked store gives the 600 ms condensed version every time, never the 7 s one in a loop. A corrupt value reads as empty and is rewritten by the next add. The id is marked seen when a run *starts*, so leaving mid-reveal does not replay it. Moving "seen" to the server (M8 as written) is left for when the profile API exists.

**Which moments.** The climax is the player's own judged moves only (`view.moments`, worst first by the drop, T3), the top three played third worst first. The prologue's `n` counts every judged ply, because every judged ply has a graph marker.

**Times** (`reveal/schedule.ts`, verbatim from §1).
- Full: ring stagger `min(80, 360 / (n − 1))` (80 when n ≤ 1); beats at `1200 + i × 2060`; only the last does not retract; total `1200 + (k − 1) × 2060 + 1900` (7220 for three), or 600 with no moments.
- Condensed: stagger `min(40, 360 / (n − 1))`, no beats, total 600; the worst moment with its marks from 0.
- Within a beat the board shows the position before the move from t0, plays the move at t0 + 400 (the board's own lift, travel on the piece's `settle` and `land` bring it to contact near t0 + 760), draws the moment's marks from t0 + 860 (the annotation layer's own ring, arrow and badge timing, M2 and M3), and retracts them at t0 + 1900.
- During the prologue the board holds the end position without marks. That is also what the server renders, so hydration never swaps the board.

**One clock.** `reveal/use-reveal.ts` runs one `requestAnimationFrame` loop per run (it reads the view through a ref, so a new view object, such as a router refresh, never restarts or re-runs it) that reads `performance.now()` against the run's start and derives the frame from the schedule; React state changes only when the frame does. There are no `setTimeout` chains. Skipping is one assignment (the start goes to −∞, so every tick is past the end), followed by one tick so the end state shows in the same event.

**Interruption (M7).** Any `keydown` or `pointerdown` (on window, capture phase, never `preventDefault`) skips, and the same event still does its own job from the end state. ArrowRight during the reveal skips *and* steps to the move after the worst; a press on a ledger row skips and the click goes to that row. Escape is a key, so it skips. The end state is the worst moment with its marks. The review's own state already sits there (its opening ply is the worst moment), so nothing jumps when the reveal hands back.

**Controls.** While a full run plays, a quiet "Skip" button sits right-aligned in a header row above the board (M8), never over it. On a phone (below 768px) it sits instead at the end of the move line under the board, a line that is already there, so the board and the verdict keep their first-screen places. Both slots are in the server HTML for every analysed game, and they keep their size when empty (under reduced motion too), so a button arriving or swapping never moves the board (fix round 1, I1). Afterwards the same place holds "Replay analysis", which re-runs the full version and first puts the review on the worst moment (silently, not a user step). The two buttons have different React keys, so a press that skips can never land as a click on Replay; if the one with focus is replaced, focus moves to the other. The condensed version is under a second, so it shows no Skip (M8). **Amendment to M8:** M8 says the control disappears when the sequence ends; here it becomes the persistent "Replay analysis" (the brief and R10), and DESIGN.md M8 is to be read with this ADR.

**A still column.** The verdict reserves three lines from 768px and the green-arrow caption one line, so the column beside the board does not jump from beat to beat or step to step. During a beat the caption names the green arrow only once that beat's marks are drawn (fix round 1, I2).

**Announcement.** The page-level polite status (ADR 0051) says "Analysis ready" once, when the first run on the page ends; a replay's end does not repeat it. The step announcer follows the review's own ply, not the reveal's frames, so nothing is read mid-reveal. It reads a text only when it differs from the last one read, so an effect re-run (React StrictMode, the end of a reveal) never re-reads the shown move.

**Exits.** No reveal runs, and the page opens on its usual end state, when:
- the URL has `?ply=` (a shared or reloaded position: the reader asked for that move);
- reduced motion is on (M10). It is read from the media query itself when deciding, because during hydration `useReducedMotion` still holds the server's guess of full motion; a reduced-motion user never gets a run that starts and is skipped;
- the game has no analysis (waiting, failed or skipped). A pending game keeps T10's honest state: the "Analysing" placeholder, no eval bar, no marks, no invented numbers and no reveal. It is not marked seen, so its first view after analysis finishes gets the full reveal.

Under React StrictMode the run is decided once per component (a ref), so the second effect pass does not find the id it just stored and run the condensed version on a first view (found on the lab run, now tested).

## Consequences

- **Deviations from §1:** the prologue uses `@ci/ui` EvalGraph's existing `drawIn` (one wipe over 2 × `--dur-reveal`, marks fading in as it passes), not a 600 ms line draw followed by per-ring strokes on the computed stagger; `ringStagger` is computed and tested, but the graph does not take it yet. Retracting marks are removed rather than reversed over `--dur-quick`, and Skip completes the graph at once rather than over `--dur-quick`. The scoresheet row's glyph fade and ring are not choreographed. These need `@ci/ui` changes (an EvalGraph ring-stagger prop and an annotation retract), left for T13 or a ui follow-up.
- The game id now reaches the client view (it is already in the URL). The rest of the view model stays narrow (ADR 0054).
- No live update: the page does not listen for `game.analysed` yet (M4 T23's stream is not wired to this page), so a pending game shows its reveal on the next visit after it finishes, not in place.
