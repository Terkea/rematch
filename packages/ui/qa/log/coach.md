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
