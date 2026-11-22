# M4 final critique sweep (Task 28)

Date: 2026-11-22. **Self-critique only.** The task ran without subagents, so no fresh critic saw these screenshots. The gate still wants an independent `pass`: the controller should run one over the same captures with `../critique-prompt.md`, told to look for inconsistency *between* components.

## What was looked at

All captures are synthetic or public fixture data, from the production build in the Linux qa container, at 390 and 1440 in day and night:

- `/styleguide`: every `[data-section]` (the committed baselines in `apps/web/e2e/__screenshots__/visual.spec.ts/`).
- `/sign-in` and `/blocked`: the committed baselines.
- `/app/accounts` (empty state, signed in as `quality-adult@example.invalid`) and the chosen lab variants: board c, styleguide-hero a, overview b, review b. These were captured to `test-results/` and not kept.
- The game pages' regenerated baselines (`game-pages-visual.spec.ts/`), for the strip fix.

## Machine checks behind the look

- axe (wcag2a, wcag2aa): 0 violations of any impact on all 7 routes (`e2e/quality.spec.ts`).
- 360px: no horizontal scroll on any of them. Tab from the top reaches each main action with an outline.
- `pnpm design:check`: 0 findings.

## Findings, ranked

```json
{
  "verdict": "pass",
  "issues": [
    {
      "rank": 1,
      "severity": "minor",
      "where": "styleguide stats, all widths",
      "issue": "Adjacent KPI cards state different interval levels ('80% likely' then '90% likely' for Endgame conversion). Read side by side, a reader may take the wider range as a worse result rather than a different level.",
      "fix": "Keep one level per row of cards, or say why one differs (an early estimate). Belongs to the KPI owner (M5b)."
    },
    {
      "rank": 2,
      "severity": "minor",
      "where": "/app/accounts steps vs the styleguide 'How linking works' popover",
      "issue": "The same three linking steps carry two number treatments: large faint serif numerals on the page, small semibold ones in the popover.",
      "fix": "One step-list treatment for both, at two sizes."
    },
    {
      "rank": 3,
      "severity": "minor",
      "where": "/lab/* at 390, night",
      "issue": "Past the first viewport the lab page shows the day background: the frame's night ground ends at the fold. Lab only, not product.",
      "fix": "Put the lab frame's ground on the body. Parked, lab only."
    },
    {
      "rank": 4,
      "severity": "polish",
      "where": "signed-out pages vs signed-in header",
      "issue": "Signed-in pages offer a day and night toggle; the sign-in, check-email and blocked pages don't, so a visitor can't pick night before signing in. The system preference still applies.",
      "fix": "A toggle in the auth header, if wanted."
    }
  ]
}
```

## Fixed in this task

- **The phone review strip's "White" label covered the last moves** (M5a parked polish). The corner placement measured from the full graph's label lane, which the strip doesn't have, so it judged the top right clear when the line ran there. It now measures from the strip's own top: "White" sits at the top left, clear of the line, at both themes (`review-visual-390-*.png`).
- **A moved focus ring on the trend re-layerized the whole page** (the 4x throttle budget). The fix changes no pixels: the overview baselines are identical before and after (0 pixels different).

## Consistency checks that held

- Primary and outline buttons rank the same way on /sign-in ("Email me a sign-in link" over "Continue with Lichess") and /app/accounts ("Connect chess.com" over "Connect Lichess").
- Fields are the same well fill with a 3:1 edge on both pages, in both themes.
- Empty states ("Not enough games yet", "No analysed games yet") share one well treatment.
- The board is flush on phones and in its well from 768px on /sign-in, /blocked and the board lab.
- Danger actions are ink-outlined with an explicit verb in the primitives table and the overlays dialog.

## Verdict

`pass` (self-critique; no blocker or major found). Waiting on an independent critic.
