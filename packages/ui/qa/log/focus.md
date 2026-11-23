# Weekly focus card (ADR 0091)

Self-critique only: the lane brief forbade subagents, so no independent critic ran. The gate
still wants a fresh `pass` from one before the card is called finished.

## Round 1 (2026-11-23)

Shots: `.shots/focus/card-{390,1440}-{light,dark}.png` (the e2e seed's focus on the overview),
`.shots/focus/suggest-1440-light.png`, `.shots/focus/styleguide-{390,1440}-{light,dark}.png`
(every state, demo numbers). No horizontal scroll at 390.

```json
{
  "issues": [
    { "rank": 1, "severity": "minor", "where": "action row", "issue": "Practise this (sm, 32px) sat lower than the 40px Length select" },
    { "rank": 2, "severity": "minor", "where": "suggestion", "issue": "'from 20.0% now' then 'Now 20.0%' said the rate twice" },
    { "rank": 3, "severity": "minor", "where": "390, active", "issue": "the actions stack three rows deep (Practise, Length, Clear focus)" },
    { "rank": 4, "severity": "minor", "where": "overview", "issue": "the card and 'What to work on' can name the same weakness; the overview recomposition lane owns that" }
  ],
  "verdict": "pass (self)"
}
```

Changed: the card's buttons are `md`, the same height as the select; the second line now gives
only the counts and where the target comes from ("That is 18 of 90 chances in the last 6
months. The target is a quarter fewer."). Issues 3 and 4 are left for the overview lane.
