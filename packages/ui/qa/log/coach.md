# Ask your coach `/app/coach`: critique log (M5b Task 9)

Screenshots are not committed. They were taken in the throwaway compose project `ci-m5b-t9` (no
host ports): `next dev` of the worktree against `chess_e2e_test`, signed in as the synthetic e2e
user `adult@example.invalid`, with reduced motion, at 390 and 1440 in day and night. They are in
the session scratchpad, `m5b-t9/`.

Data: the e2e seed (`seedE2eGames`: three public fixture games of a public Lichess account and
synthetic games), `game_metrics` rows for its analysed games, and synthetic threads written with
Task 5's `seedThread` and `seedChunk`. The answers are labelled "Sample answer" and talk about the
real fixture game DrNykterstein v Night-King96 (31… Nf4, +0.1 to +4.0; 33. Nc3 Bxc3). No owner
data. The web ran with `AI_DEFAULT_PLAN=free` (3 a week) so the allowance states are reachable
with a few rows.

## States captured

- `/app/coach`: five threads (a general question, an anchored move, three more), the scope switch
  (three options), "New conversation", the allowance line and the disclosure note.
- `/app/coach/new?anchor=<move>`: the anchor card (the position after 31… Nf4 as a small board,
  the game line, a link to the review at that ply), the composer and the disclosure.
- A thread answered: the question, the answer with its disclosure, two paragraphs, linked moves
  and the engine check "31… Nf4 32. Bxf4 Qxf4 (done)".
- A thread mid-answer: the interim engine line in pencil, two streamed chunks, "Your coach is
  still writing", the composer disabled with "Your coach is answering. Updates every few seconds"
  (no event stream is mounted on this base, so the page polls and never says "live").
- A thread writing (pending, nothing streamed yet).
- A failed answer, unavailable, with "Ask again".
- A failed answer, the spending limit (no retry).
- The allowance used: the composer disabled with the cap copy and the reset day.
- Not found (an unknown id).

Each capture is checked for page-level horizontal overflow and for `NaN|undefined|Infinity|null|{{`
in the visible text.

## Pass 1 (self-critique only)

The implementer was told not to dispatch subagents, so this is self-critique only; **the
controller owes the independent critic pass** (the reviewer gate needs `pass`).

Found and fixed:

1. **The anchor board overflowed the phone (major).** The board takes the page-wide
   `--board-size`, so in the card it was wider than the screen (37px sideways scroll at 390). The
   card's board column now sets `--board-size` from its own container width in whole-pixel
   squares (`round(down, 100cqi, 8px)`, minus the 8px well from 768px), as the weakness exemplars
   do.
2. **"Questions are paused until they reset" while the coach was answering (major, honesty).**
   The composer's disabled reason is for a used-up allowance only; an answer in progress is now a
   status line under the box ("Your coach is answering", plus "Updates every few seconds" without
   an open stream).
3. **The composer rule overhung the column** at 1440 beside the thread rail; it now spans the
   column only.
4. **"this week , resets"**: tabular figures widened the comma in the allowance line; removed.
5. **"Delete conversation" read as stray text** on the phone (a quiet button indented by its own
   padding); it is now a small outline button with a trash icon.

## Pass 2 (self-critique only, retaken after pass 1)

Shots: `m5b-t9/{list,new,not-found}-{390,1440}-{light,dark}.png`, the thread states as
`m5b-t9/{answered,streaming,writing,unavailable,budget}-{390,1440}-{light,dark}-{top,end}.png`
(viewport shots, because the composer is pinned to the viewport's foot), and the allowance used as
`m5b-t9/exhausted-{exhausted,list}-*.png`. Every capture: no sideways overflow (0 at 390), no
`NaN|undefined|Infinity|null|{{` in the visible text. Console: a 404 for the not-found page, and
`next dev`'s "script tag while rendering" warning from the theme script on full loads, which every
app page has (not this surface).

Found and fixed:

6. **The "Written by AI" pill broke over two lines** beside the disclosure note at 390; it no
   longer wraps.
7. **The scope switch on a new conversation opened from a move** would have dropped the anchor;
   it is hidden when an anchor is shown (the anchor belongs to one account's games).

No blocker or major left in the self-critique. Verdict: pass (self-critique only; the controller
owes the independent critic).

Open for the critic:

- Whether the thread rail (1440, other conversations) earns its column.
- The pending answer is a `ProgressPlaceholder` list ("Your coach is reading your games"): is it
  too tall for a chat turn? (Parked from Task 3's critic as kit polish.)

## Pass 3 (review-t9 fixes, self-critique only)

Shots: `m5b-t9/fix1/` (same names as pass 2, plus `new-plain-*` for a new conversation without an
anchor). The stack now mounts the event stream (main's T27 wiring). The streaming thread is seeded
with a released first attempt (attempt 1, seq 0) and the retry's chunks (attempt 2, seq 0 to 2).
Only the retry's text shows: the replay keeps the latest attempt.

Fixed from the review's visual pass:

- **V1:** the pending answer is one line at answer scale, a dashed pencil ring and "Reading your
  games", in place of the kit's heading-size title over a skeleton card and "Writing". The
  composer's status line reads "Your coach is writing", one wording.
- **V2:** stored prose is set at the streamed size (`text-prose`, 18/30), so a finished answer no
  longer shrinks. The cause is in the kit: `SegmentText` merges `text-prose` and `text-ink`
  through `cn`, and tailwind-merge keeps only the last `text-*`. That is a T3 follow-up.
- **V3:** on a phone the box and Send share one row, the empty box is one line, and it grows as
  you type. The allowance drops its bracketed day while open, and the status line replaces it
  while an answer runs. The placeholder is now "Ask about your games", so the empty box stays one
  line high. Measured composer: see the report.
- **V4:** the scope switch scrolls in its own row with T6's right-edge fade below 640px (list and
  new pages).
- **Polish taken:** the month window reads "resets on 1 Dec 2026" (no brackets), and "Ask again"
  is offered on the latest answer only. The others are left for the critic and T11.

## Fix D (2026-11-22): cited boards under answers, units, hint, labels

Self-critique only (the lane ran without subagents). Shots of the styleguide's coach answer at
360, 390 and 1440, day and night (`.shots-fixd/cited-*.png`, not committed); no horizontal
scroll at any width.

- The answer that cites 28. Nd2?? shows one board under its paragraph: the position before the
  move from White's side, a pen arrow for the blunder, the key "Played, Nd2" and the link "Open
  the game at 28. Nd2". With an engine move it adds a `good` arrow and "Better, <move>".
- The board sits in a 192px column (208px from 768px): a reference diagram, not a second review
  board. A one-square knight arrow is short but stays legible on its halo in both themes.
- Numbers read with their units ("86.5%", "40.7 s"); the composer hint reads "Shift + Enter"
  once; copied text reads "better move: Qe3". No visual change for the last two.
- Verdict: pass (self). Open for the critic: whether the board should grow at 1440.
