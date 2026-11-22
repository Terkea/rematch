# M5a final sweep: overview, games, review (Task 13)

Date: 2026-11-22. Surfaces: `/app`, `/app/games`, `/app/games/<id>` (and `/styleguide` for the trend).

## Screenshots

- The committed visual baselines, `apps/web/e2e/__screenshots__/game-pages-visual.spec.ts/{overview,games,review}-visual-{390,1440}-{light,dark}.png`. They are made in the Linux `qa` container from the `e2e-fixtures` seed: public fixture games (a public Lichess account) and synthetic ones with fixed dates and fixed game ids, so the seeded bootstrap bands are the same on every run. No owner data.
- The review is captured on the player's `??` (`?ply=62`); the overview and the games list as one tall viewport (Chromium's `fullPage` capture caught the trend mid-measure and blank at 1440, the same failure T4, T5, T7 and T10 reported).
- The owner walkthrough ran on a lab copy of the owner's games (compose project `ci-m5a-t13`); those pages were checked by script and not captured.

## Round 1: self-critique (no independent critic)

The implementer was told not to dispatch subagents, so this round is a self-critique against `../critique-prompt.md`, looking for inconsistency between the three pages and with `/styleguide`. The reviewer gate still needs a fresh critic's `pass`.

```json
{
  "issues": [
    { "rank": 1, "severity": "major", "screenshot": "overview-visual-1440-*.png (first capture)",
      "observation": "The first 1440 baselines had a blank 'Accuracy over time' plot, and no band legend, while the 390 ones drew it. The DOM held all the dots: Chromium's fullPage capture resizes the view and the trend (which measures its parent) was captured mid-measure.",
      "fix": "The visual spec waits until the trend's clip is the plot's full width, then captures one tall viewport instead of fullPage (the T10 approach). The retaken 1440 shots draw the trend (T4's deferred charts-1440-dark retake)." },
    { "rank": 2, "severity": "major", "screenshot": "overview-visual-1440-light.png vs -dark.png (first capture)",
      "observation": "The 80% bands differed between two runs of the same seed (65.1–71.3% vs 65.1–71.4%; blitz accuracy 66.7–75.4% vs 66.6–75.7%). The bootstrap is seeded by game ids, and the seed used random ids.",
      "fix": "The e2e seed gives every game a fixed id. Not a product bug: the same games always give the same band, and new games are meant to move it." },
    { "rank": 3, "severity": "minor", "screenshot": "review-visual-1440-light.png",
      "observation": "'2 s on this move' sits under the side to move (White, 1:06) while the move shown is Black's 31... Nf4?? (T5 M6, parked since T5).",
      "fix": "Not changed here (out of T13's list); carried as a concern for M5b." },
    { "rank": 4, "severity": "minor", "screenshot": "games-visual-1440-light.png",
      "observation": "The From and To date fields show the browser's own 'mm/dd/yyyy' placeholder, while every other date on the three pages reads '31 Oct 2026'.",
      "fix": "Native date input, locale-driven; left as is (a custom date field is M5b work if wanted)." },
    { "rank": 5, "severity": "minor", "screenshot": "review-visual-1440-light.png",
      "observation": "At 1440 the board column starts about 40px below the verdict column: the reserved slot for the reveal's Skip/Replay control (T11, kept for zero layout shift) is empty on a ?ply= view.",
      "fix": "Accepted: the reservation is what keeps CLS at 0.002 during the reveal." }
  ],
  "consistency": [
    "Dates read '31 Oct 2026' on the overview's Recent games, the explorer's day headings and the review's meta line.",
    "Severity marks are graded the same way everywhere: ?? in pen, ? lighter, ?! ink, on the explorer's Marks column, the review ledger, scoresheet, graph and board badges.",
    "n is stated with every rate: the headline ('Across 34 analysed games'), each class ('Based on 17 games in blitz'), the mistakes rows ('Across 34 analysed games'). Bullet (1 game) shows a dash and 'Need 9 more games', never 0.",
    "The trend, sparklines and judgement columns use the styleguide's chart encodings (filled win, half draw, ring loss; pencil band)."
  ],
  "verdict": "pass (self-critique only)"
}
```

## Checks

- Quality loop (`e2e/game-pages-quality.spec.ts`): `/app`, `/app/games`, an analysed review, a pending review and the 300-ply review at 360 and 1440: 0 px sideways overflow, a visible focus outline (or the board's drawn focus ring) on every tab stop up to the page content, and no `NaN`, `Infinity`, `undefined` or `null` in the visible text.
- Trend (`e2e/charts.spec.ts`): a keyboard step on the 500-game styleguide trend, at 4× CPU throttle on a production build, median 8.0 ms, p90 10.7 ms, max 14.3 ms (budget 16 ms); `next dev` is about ten times slower (median 110 ms), so the test needs `E2E_PRODUCTION=1`. A touch tap opens the nearest game and Previous / Next step. WebKit draws the trend (clip at full width, 500 dots).
- `pnpm design:check`: 0 findings.
