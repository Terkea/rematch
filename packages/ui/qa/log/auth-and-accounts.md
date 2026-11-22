# Auth and accounts screens (Task 21)

Date: 2026-11-22. Surfaces: `/sign-in` (plus `?error=link_expired`), `/sign-in/check-email`, `/welcome/age`, `/blocked`, `/app` (Dashboard placeholder), `/app/accounts` in its empty, pending, verified, command-menu and remove-dialog states.

## Screenshots

These are at 390 × 844 and 1440 × 900, day and night, with reduced motion and full-page captures. They are not committed. The copies are in `.superpowers/sdd/2026-11-21-04-web-foundation-and-design-system/shots-t21/` as `<screen>-<width>-<theme>.png`. Every capture measured 0 px horizontal overflow.

Authenticated states use the e2e storage state (`apps/web/e2e/.auth/adult.json`). The pending and verified states were driven through the real routes against the chess.com fake.

## Round 1: self-critique against `../critique-prompt.md`

The implementer was told not to dispatch subagents, so this round is a self-critique. An independent critic should still run in review.

```json
{
  "issues": [
    { "rank": 1, "severity": "major", "screenshot": "accounts-pending-1440-light.png",
      "observation": "Two ink-filled primary buttons in one view: 'Check again' and 'Connect chess.com'. DESIGN.md allows one primary action per view.",
      "fix": "While a code is pending, 'Check again' is the primary action; 'Connect chess.com' drops to outline." },
    { "rank": 2, "severity": "major", "screenshot": "sign-in-390-light.png",
      "observation": "On a phone the form is centred in the viewport, leaving about 250 px of empty paper between the wordmark and the title.",
      "fix": "Top-align below lg (pt-14, sm:pt-24); centre only beside the diagram from lg." },
    { "rank": 3, "severity": "minor", "screenshot": "accounts-empty-390-light.png",
      "observation": "With no accounts, the empty 'Linked accounts' sheet comes first on a phone and pushes the only useful action below it.",
      "fix": "With nothing linked, the connect panel goes first below lg." },
    { "rank": 4, "severity": "minor", "screenshot": "welcome-age-1440-dark.png",
      "observation": "The privacy hint sits tight under the two-line heading (negative margin).",
      "fix": "Remove the negative margin and use the fieldset gap." }
  ],
  "verdict": "revise"
}
```

All four were fixed in the same task. The screens were re-shot, and the same checks then found no blocker or major issue.

## What carries the direction

- **The board does the talking.** From lg, sign-in, check-email and the age screen show the Opera game after 9…b5? in a `sheet`: a pen ring and a `?` badge on b5 draw themselves on load. The caption "9…b5? +5.7" uses the real Morphy game (B15), with the `?` in `pen-text`. Everything around it stays quiet and tonal.
- **Type.** Literata 600 for titles, panel headings, account names and the verification code. Schibsted for the UI. Sentence case throughout, no eyebrows (B5).
- **Motion, each with a cause (M1):**
  - the ink rule under the current nav item slides on `page`;
  - incoming pages turn in from their right edge (choreography 5, client navigations only);
  - the envelope lands on `land` on check-email;
  - a newly verified badge lands on `land`;
  - pending buttons soak an ink line along their foot instead of spinning (B18);
  - rows enter and leave with opacity and scale only (M9).

  Under reduced motion all of these are instant or opacity-only (M10).
- **Red** appears only on the chess mistake (the diagram) and on form errors (`pen-text` with `WarningCircle`). The destructive "Remove account" is ink-outlined (the `danger` variant).

## Verdict

Pass after round 1 (self-critique). Open for review:

- an independent critic's pass;
- the outgoing half of the page turn, which waits for the View Transitions work.
