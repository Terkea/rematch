# Filter bar, colour split and period comparison (T3)

Self-critique only: the lane brief allowed no subagents, so no independent critic saw these.
The gate still wants a fresh `pass` before the surface is called done.

## Round 1 (2026-11-22)

Shots (signed in, synthetic seed `e2e/support/seed-filters.ts`), written by `e2e/filters.spec.ts`
to `.shots/filters/<route>/<width>-<theme>.png`: overview-compare, overview-colour, overview-short,
games, progress and weaknesses at 390 and 1440, day and night. No sideways scroll at either width.

```json
{
  "verdict": "pass-with-minors (self)",
  "issues": [
    { "rank": 1, "severity": "minor", "where": "colour split and compare detail lines",
      "issue": "Sans range lines used tabular figures, so '64.2%' spread out against the prose",
      "fix": "Proportional lining figures for sans text; tabular stays on the serif values" },
    { "rank": 2, "severity": "minor", "where": "custom range and custom periods",
      "issue": "Native date inputs show the browser locale (09/01/2026 in the en-US qa browser), which the M5a T13 critic disliked on the games page",
      "fix": "Kept: the brief asks for native inputs and the GET form must work without JavaScript; ADR 0083 amendment 1 records it" },
    { "rank": 3, "severity": "minor", "where": "Compare periods group with Custom open",
      "issue": "Stop comparing wraps to the far right when the custom form is open",
      "fix": "Left as is: it stays in the group and in reading order" }
  ]
}
```

Changed: figures (issue 1). The White | Black row stays quiet (body weight, serif values at `lg`),
below the one headline; the change column holds only verdict words, the difference beside a
reached verdict only, and "Not enough games to tell: need 6 more in period B" below the minimum.
