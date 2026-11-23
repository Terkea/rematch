# 0094. The coach knows the player's plan: a per-user plan context

Date: 2026-11-23. Status: accepted (lane `coachplan`, owner-approved plan Part 3 item A). Builds
on ADR 0086 (the coaching-input snapshot), ADR 0090 (the trainer), ADR 0091 (the weekly focus),
ADR 0093 (the validator's honest wording) and ADR-FIX-N-1 (one verified reader per account).

## Context

The chat, the weekly review and the report had no reference to the player's weekly focus or
their trainer practice, so the coach could not say "you practised on 4 days; missed mates went
from 14% to 11%". The focus and the trainer are per user (`focuses.user_id`,
`drill_items.user_id`), while the coaching input (ADR 0086) is per account and time class,
built by `build-profile` and read by every user linked to the account.

## Decision

1. **A per-user plan context, not per-user snapshots.** `ai::plan::PlanContext` is built when
   a text is written (`ai::plan::load_plan(user, account, time class, now)`), next to the
   account's coaching input, and never stored in `coaching_inputs` or `report_runs.input`. A
   per-user snapshot would multiply the build-profile work by the users of an account and go
   stale between builds; the plan changes with every attempt, and its queries are small.
2. **Contents** (every rate with its n and its 80% Wilson range, shares in 0..1, dates
   `YYYY-MM-DD` UTC):
   - `focus`: the running focus of this account and time class: series key, plain name, side,
     start, `targetRate`, `gamesN`, `gamesPlayed`, `progress` (events of chances, rate, low,
     high), `status` (`on_track`, `not_yet`, `too_early` with `moreChances`), `outcome` once it
     has its N games, and the baseline. The counts are the web's `focusCounts`; the verdict
     rules are `focusProgress` (10 chances).
   - `training.lastWeek` and `training.lastMonth` (7 and 30 UTC days, today included): days
     practised, attempts, right answers and `firstTryRate` (the trainer records only the first
     attempt of an item a day, so every attempt is a first try), per source, and the positions
     retired; plus `activeItems`.
   - `isItWorking` per source (before against after the source's first drill) and `patterns`
     (drilled against held-out motifs), with the web's `verdictOf` (30 chances, separated
     ranges, else `too_early` or `no_clear_change`).
   The arithmetic is a port of `server/focus/goal.ts` and `server/train/model.ts`
   (`ai::plan::compute`), so the coach quotes what the cards show. `None` when the user is not
   linked to the account or has neither a running focus nor a trainer position there.
3. **Isolation.** Every row is found by the user's id, and the user must hold a link to the
   account. User B on a shared account never receives A's plan (tested), and a user without a
   link gets none even if a row names the account.
4. **Chat.** The chat job builds the asking user's plan (the conversation's `user_id`). The
   block `<plan_context>` (the plan plus its `refs`) rides with each question, not in the cached
   coaching context: it changes daily, and a resumed SDK session receives only the new
   question. Its paths join the turn's evidence, and the answer stores the plan it was written
   from (`coach_messages.plan_context`, migration `20261123060000_coach_message_plan.sql`), as
   it stores its input (ADR 0086 §4). A failed plan load is logged and the turn goes on without
   it.
5. **Weekly review.** A review is the account's, and ADR-FIX-N-1 gives an account at most one
   reader of its AI artefacts: the verified owner. The weekly schedule attaches that owner's plan
   to the input (`WeeklyReviewInput.plan`, optional, stored in `weekly_reviews.input`); an
   account without a verified owner gets none (so the dev bypass `ALLOW_UNVERIFIED_LINKS`
   reviews carry no plan). The training counts are process numbers (the process text may cite
   them); the focus progress and the "Is it working?" numbers are outcome numbers, which the
   non-outcome items may not quote, as for `focusUpdates`. The web already prints the computed
   "Your focus this week" line beside the review (ADR 0091 §8).
6. **The report leaves the plan out.** A stored report is read for weeks while the plan moves
   daily, it is account data shared with whoever verifies the account later, and its climb plan
   is a diagnosis, not a progress check. The chat and the weekly review carry the plan.
7. **Prompts.** `chat/system` v5 and `weekly-review/system` v2 add a short section: use the
   plan when the player asks how they are doing, acknowledge practice with its tokens, connect
   the focus rate to its goal, suggest the next drill, say "too early to tell" for `too_early`,
   claim a change only for `lower`/`higher`, and invent nothing. The weekly prompt text is now
   read at its set version. v4 and v1 stay for stored hashes.
8. **Validator.** `Evidence::add_plan` indexes the plan under `plan.` (per-source arrays keyed
   by `source`: `plan.isItWorking.W4_missed.after.rate`) and marks every fraction
   (`rate`, `low`, `high`, `targetRate`, `baselineRate`, `firstTryRate`) as a rate path, so it
   always prints as a percentage. `tests/plan_context.rs` walks the plan's JSON Schema and fails
   on a fraction that renders bare.

## Consequences

- The coach can say what the player practised and whether it shows, from cited numbers, and
  says "too early" when the sample is thin.
- Each chat turn runs a few small queries more (the user's items and attempts, the account's
  `game_metrics` rows; the move scan only for a user with blunder items). The trainer's
  stricter queries from the practice-performance lane can replace these later.
- The Rust port must follow the web's focus and efficacy rules; a rule change there needs the
  same change in `ai::plan::compute`.
- Days are UTC, as the trainer's schedule is today.
