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

## Critic pass 1 (independent, screenshots only)

Judged from `coach-{390,1440}-{light,dark}.png` in the session scratchpad (`m5b-t3/`). I formed this view before reading the log above. Locations are given as the section and demo label in each capture.

**Verdict: needs-changes.** 0 Blocker, 6 Important, 8 Polish.

What works: the prose voice is right. It reads as serif running text, with no bubbles around the coach's answers. The notation is bold and underlined, the better move gets a green head, and engine numbers get a dashed pencil underline. The pages look like a coach's annotated notes, not a ChatGPT clone. The "Written by AI" chip plus date is visible without shouting, and the full note is plain and honest ("It can contain mistakes. The engine data ... is the source of truth"). Text contrast is good in both themes. There is no horizontal overflow at 390, and the 16px gutters hold. The failed and allowance copy is honest: "this question was not counted" and "They reset on 16 Nov 2026".

### Blocker
None.

### Important
1. **The engine-check line uses the wrong move numbers.** Conversation, done answer ("Engine check: **1. Nd2 e4 2. Nxe4** (done)") and streaming ("Engine check: 1. Nd2 e4 2. Nxe4 (pending)"), all four captures. The same line is "28. Nd2 e4 29. Nxe4" in the streaming pencil aside directly above. The engine-check chip seems to number from 1 instead of the real ply. A chess player reads this as a wrong line.
2. **The done and pending engine checks use the same icon.** Same rows as item 1. Both use the dashed, spinner-like ring, so a finished check still looks in progress, and "(done)" in small grey text is the only signal. Give the done state a settled mark (for example a check or a solid ring) and keep the dashed ring for pending.
3. **"Building 2 / 5 sections" shows no written sections.** States > "Building, with a count", all captures. The subtitle says "Sections appear here as they are written", yet all rows are skeletons and there are 4 rows, not 5. The count claims progress that the list does not show. Show 2 written rows and 3 placeholders, or at least 5 rows with 2 marked done.
4. **The ready composer looks disabled.** Composer > "Ready", light theme mainly (`coach-1440-light.png`, `coach-390-light.png`). The textarea has the same grey fill as the info callouts, and Send is a grey ghost. Before anyone types, the ready box and the "Allowance used" box are nearly the same. Give the ready field a surface or raised fill and a clear border, so the disabled one can recede.
5. **The allowance-used composer still invites typing.** Composer > "Allowance used", all captures. The placeholder still reads "Ask about your games", and the only difference from Ready is a fainter border plus the reason line below. Change the placeholder (for example "Questions reset on 16 Nov") or show the box in a visibly locked state such as dimmed text, a lock or clock glyph, or `not-allowed`.
6. **Touch targets and the send row at 390.** In `coach-390-*.png`, "Try again" (States > Failed) and "Open your games" (States > Empty) are about 32px tall. That is under 44px, and unlike the info button they have no larger hit area. On the ready composer the Enter / Shift+Enter hint wraps and pushes Send to its own row. Phones have no Enter-to-send, so hide the hint on coarse pointers and keep Send next to the field or right-aligned on the field's row.

### Polish
1. **The skeleton list is misaligned.** States > Building / Writing and Conversation > "Waiting for the first words", all captures. The ruled list is inset about 20px from its heading and subtitle, and the bottom rule is shorter than the rows above it.
2. **Dark-theme skeletons are hard to see.** In `coach-*-dark.png`, the skeleton bars are darker than the page (they look sunken), the row rules almost vanish, and the "Writing" track under the placeholder is almost invisible. Lift the bars and track a step above the background.
3. **"Play stream" is bare text.** Conversation > Streaming, right-hand side. It floats at about x=700 at 1440 and has no button affordance. Even as a demo control it should look clickable.
4. **The conversation's writing placeholder is too tall** (also an open question in the implementer's log). Conversation > "Waiting for the first words". An xl title plus a 4-row ruled skeleton sits under a one-line question. Inside a chat, a single line ("Your coach is reading your games" plus "Writing") is enough. The heading says "reading" while the label says "Writing", so pick one verb.
5. **"engine check" repeats.** Segments and Conversation answer. Agreeing with the implementer, two labels in one sentence read busy, and at 390 "−1.1 engine check" wraps onto its own line. Keep the label on the first engine number and let the dashed underline carry the rest, or put the label in the tooltip.
6. **The streaming tail is unstyled.** Conversation > Streaming, "Once it left, Black pushed e4 and". The "e4" here is plain text, while every other "e4" is bold notation, so the text will visibly re-style when the segment commits. There is also no live caret or cue on the tail apart from the grey "Your coach is still writing" line below.
7. **The user question box.** Conversation > "You asked". The filled grey rounded box is the one place that leans toward a chat-bubble pattern. It is acceptable because it is left-aligned and labelled, but a quieter treatment (a left rule or quotation styling) would fit the coach's-notes voice better.
8. **Callout weights.** States grid. The report-failed and allowance-used callouts share the same border and fill, while Unavailable and Computed only have no border. That is fine, but "failed" is no stronger visually than "allowance used". Consider a slightly warmer tone for failed, without turning it red.

### Notes on the implementer's open questions
- Writing placeholder in a conversation: yes, it is too tall (Polish 4).
- Repeated "engine check": yes, it reads busy (Polish 5).

## Fix round 2 (after critic pass 1 and the code review)

Shots: `m5b-t3/fix2/coach-{390,1440}-{light,dark}.png` in the session scratchpad. No console errors, no overflow, no bad text.

Critic Important:

1. Engine-check lines number from the real ply (`startPly`, e.g. "28. Nd2 e4 29. Nxe4", "28… e4"); without a ply they are left unnumbered, never restarted at 1.
2. Done checks carry a check-circle, pending the dashed pencil ring, failed a warning circle; labels "(done)", "(checking)", "(failed)".
3. "Building 2 / 5 sections" lists all five sections by name, the two finished ones with the success check (`ProgressPlaceholder` `items`, list shape only).
4. The ready composer is an open `sheet` field with a `rule-strong` edge; disabled recedes to `well` with a `rule` edge.
5. A used-up composer's placeholder is "Questions are paused until they reset", with the reason ("You have used this week's 100 questions. They reset on 16 Nov 2026.") in place of Send.
6. State actions are `md` buttons with the coarse-pointer 44px hit area; the composer's keyboard hint is hidden below 640px and on coarse pointers (kept for screen readers), so Send stays on the count's row.

Polish taken: "engine check" is spelled out once per run of text; "Play stream" is an outline button; the streaming tail's "e4" is a square segment.

Code review: a done answer without a stored disclosure shows the fixed "Written by AI" label; the answer's wrapper is polite only while pending or streaming, so the final text is not read twice; `hrefFor` output is limited to internal paths, fragments and http(s) (external links get `rel="noopener noreferrer"`); the count reaches screen readers 1 s after typing pauses through its own sr-only region; Enter with `keyCode` 229 (Safari IME commit) does not send; repeated notes keep distinct keys.

Not taken (polish, shared component or by brief): the dark skeleton tone and inset (shared `ProgressPlaceholder`), the tall writing placeholder in a conversation and the "reading"/"Writing" pair (brief copy), the user question's `well` treatment, a warmer failed tone.
