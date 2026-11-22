# Coach kit `/styleguide#coach`: critique log (M5b Task 3)

Screenshots are not committed. They were taken from a local `next dev` of the worktree with
reduced motion, element-clipped to `[data-section='coach']`, at 390 and 1440 in day and night:
`coach-{390,1440}-{light,dark}.png` in the session scratchpad (`m5b-t3/`). The section's demo
copy is sample text over real games (the fixture blitz game DrNykterstein v Night-King96, 28. Nd2
+2.7 to −1.1, and the Opera game, 9… b5? 10. Nxb5), never presented as a player's own data.

## States captured

Every segment kind in running prose (linked moves, the better move with its solid head, an
engine line, an opening line, a number, a date, a square, two engine checks); the full and
compact disclosure; all seven `CoachState` kinds (building with a count, writing, failed with an
action, allowance used, unavailable, computed only, empty with a link action); a conversation
(question, done answer with an engine check and a server note, a streaming answer with an interim
pencil aside, a pending answer, a failed answer); the composer ready and with the allowance used.

Each capture: no console errors, no page-level horizontal overflow, and the section HTML has no
`NaN|undefined|Infinity|null`.

## Pass 1 (implementer self-review; the fresh critic is still owed)

`pnpm design:check`: 0 findings.

The implementer was told not to dispatch subagents, so **the controller runs the independent
critic on the four PNGs and appends its verdict here** (the reviewer gate needs `pass`).

Fixed in this pass:

1. **The disabled composer kept the keyboard hint and pushed its reason to the right edge.** With
   a `disabledReason` the hint is gone and the reason sits left, under the box, in place of the
   send row.

2. **Coordinator, before review: honest progress labels.** Building and writing no longer read
   "Analysing": `ProgressPlaceholder` takes `Building` and `Writing` ("Building 2 / 5 sections",
   "Writing").
3. **Coordinator, before review: the compact disclosure on touch.** The note is now a popover that
   opens on click, tap or Enter, from a 32px button with a 44px coarse-pointer hit area.

Open for the critic:

- The `writing` placeholder inside a conversation (title at `xl` over a ruled list skeleton and
  "Analysing") is the brief's choice; it is tall next to a one-line question.
- "engine check" is spelled out after every `eng` segment; two in one sentence read busy, but the
  label is what tells the reader the number came from the engine during the answer.
