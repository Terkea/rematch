# Game review page (M5a Task 10)

Date: 2026-11-22. Surface: `/app/games/[gameId]`, in five states:
- an analysed win (as Black, a single inaccuracy);
- a loss with many blunders;
- the longest game (a long draw);
- a game opened with `?ply=` on a move judged by the lost-mate rule;
- an unknown id (not found).

## Screenshots

- Taken at 390 × 844 (touch, mobile) and 1440 × 900, day and night, with reduced motion, after hydration (a key press changes `data-ply`).
- Each state has a first-viewport shot (`-fold`) and a tall-viewport full page. Playwright's `fullPage` drops the graph's SVG, as T4, T5 and T7 reported, hence the tall viewport.
- The games are the owner's real games, in a throwaway compose project's copy of the data. The shots are not committed; they are in `.superpowers/sdd/2026-11-21-05a-game-pages/shots-t10/`.
- Every capture measured 0 px horizontal overflow, and there were no console errors.
- In the tall-viewport phone captures the sticky thumb bar sits at the end of the review, above the result lines. In a real 844 px viewport it rides at the bottom of the screen.

## Round 1: self-critique against `../critique-prompt.md`

The implementer was told not to dispatch subagents, so this round is a self-critique. The reviewer gate still needs the critic's `pass`.

```json
{
  "issues": [
    { "rank": 1, "severity": "major", "screenshot": "review-loss-390-light-fold.png",
      "observation": "The verdict started at 830 px, under the sticky thumb bar (788 px): the phone's first screen showed the board but not the sentence. The header's result, accuracy and link lines took about 90 px above the graph.",
      "fix": "On phones the result lines move to the end of the page (grid order), the engine switch moves under the verdict, and the verdict is text-xl below 768 px. The verdict now starts at 718 px, above the bar." },
    { "rank": 2, "severity": "minor", "screenshot": "review-win-390-light.png",
      "observation": "'Where it went wrong' wrapped beside its caption ('Your win chance, before to after') at 390 px, giving two ragged lines.",
      "fix": "The caption stacks under the heading below 640 px." },
    { "rank": 3, "severity": "minor", "screenshot": "review-mate-1440-dark.png",
      "observation": "With the ledger collapsed to the three worst, the shown move (a lost-mate blunder opened by ?ply=) had no row, so the ledger didn't show where the board is.",
      "fix": "The collapsed ledger keeps the shown move's row when it is one of the player's mistakes (tested)." },
    { "rank": 4, "severity": "minor", "screenshot": "review-long-1440-light-fold.png",
      "observation": "'Black (you)' still meets ticks at the left end of a long game; the solid ground keeps it legible, but it isn't clear ground.",
      "fix": "Parked: the corner label is readable on its ground. Moving the names out of the plot is a graph redesign for later." },
    { "rank": 5, "severity": "minor", "screenshot": "review-mate-390-dark-fold.png",
      "observation": "Night board: black pieces on dark squares are low contrast (the critic's T7 note).",
      "fix": "Parked for a board theme task; out of this page's scope." }
  ],
  "verdict": "pass after fixes"
}
```

## Checked against the T7 critique (binding hand-offs)

- **Layout:** variant b. The graph spans the top, and the verdict sentence is the largest text on the page (italic Literata, 36 px from 1024 px). c's ledger replaces the chip grid, with three rows plus "Show all N" above eight.
- **Severity:** `??` in full pen, `?` in a lighter pen, `?!` in ink. The opponent's marks are graphite in the scoresheet, the ledger and the graph. "Best" is on the player's moves only.
- **Graph:**
  - No bare `?` labels.
  - The long game labels only `??`, and "Middlegame" is dropped rather than run into "Endgame".
  - A 50% line and rules at 25% and 75%, with "White's win chance, White at the top" above the plot.
  - Corner names on a solid ground.
  - A scrubbable cursor.
- **Phone:**
  - A 70 px graph strip.
  - A sticky thumb bar with the previous and next mistake (labelled "Mistake"), previous, Autoplay and next.
  - The eval bar's ends are labelled "White" and "Black" by orientation.
  - The verdict starts above the bar.
- **Board mark:** a red-pen stroke from the square left to the edge of the square reached, the badge centred on the square's top corner (half outside it), and no ring. The piece stays readable at 390 px, including a one-square move into a corner square.
- **Judgements:** a lost-mate blunder reads "… is a blunder: it let a forced mate go. Win chance 98% to 90%." (synthetic figures; the real ones are in the task report) In the ledger it carries "marked for losing a forced mate, not for the drop".

## Round 2: code review and critic (review-t10.md), fix round 1

- **Board badges graded.** The player's `??` is a solid pen disc, `?` a pen outline on paper, and `?!` and every opponent mark pencil. Before, they all sampled the same red.
- **Badge off the piece.** It is centred on the square's top-right corner (top-left when an arrow arrives there), kept on the board.
- **Engine arrow named.** The switch is now "Show the engine's next move" (what it does). The always-on green arrow is named under the verdict ("Green arrow: Bxe6, the engine's choice instead.").
- **Announcer.** One page-level polite status at every width; silent while autoplaying; the scoresheet's own is off.
- **Smaller fixes.**
  - The desktop verdict is one step smaller.
  - The phone's meta line ends with ", won" or ", lost", so the first screen says who won.
  - The collapsed ledger ranks `??` first.
- **Shots.** All retaken after hydration from the fixed code. There is no horizontal overflow, and no console errors on the review pages.

Verdict: pass after fixes (pending the re-review).
