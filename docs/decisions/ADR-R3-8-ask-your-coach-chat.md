# ADR-R3-8. "Ask your coach": the chat engine

Date: 2026-11-22. Status: accepted (Task 23). Builds on ADR-R3-4 (facts by reference) and
ADR-R3-6 (the Agent SDK sidecar and its chat sessions).

## Context

The player can ask their coach questions about their own games. The answer is model text
shown to a player who may be a minor, so it must meet the same bar as the report: every chess
fact by reference, checked before it is shown, no promises and no marketing. Unlike a report
section it streams, it can call tools (including fresh engine checks), and it runs inside an SDK
session that keeps the conversation (ADR-R3-6 Amendment 1).

## Decision

1. **Where it runs.** The chat runs in the worker (`coach-chat` job, Task 25). One turn is one
   `stream_turn` with the chat tools: the sidecar runs up to `max_tool_rounds + 1` = 7 model
   rounds and calls each tool back through a per-turn handler (`chat::engine`). Rust owns the
   prompts, the tools, the validation, budgets and metering.
2. **Anchors.** A conversation or a message can be opened from a report section, a weakness, a
   position, a move, an opening line or a game. The anchor is resolved for the account
   server-side and injected as an `<anchor>` block (conversation anchors in message 1, message
   anchors before the question). Its ids feed the tools; a position anchor's FEN feeds
   `analyse_position`, and its position joins the turn's evidence.
3. **Validate before publish.** The streamed text is split on blank lines. Each paragraph is
   validated (`validate_text` with implicit citations, no required citation, and the rules
   `banned_phrases`, `no_marketing`, `engine_claims` and `no_quantity_words`) against
   `build_evidence(input)` plus the anchors plus this turn's tool evidence, all in one
   `ToolSession`. A passing paragraph is published while the stream is not held; the first
   failure holds the stream. A held answer gets ONE repair turn (no tools, nothing published);
   paragraphs that still fail are dropped and the repaired answer replaces the streamed one.
   When nothing is left, the answer is the fixed fallback sentence. Dropped statements and an
   engine-check limit are noted in fixed server copy.
4. **Off-topic.** The model answers anything that is not chess or the player's games with
   exactly `{{offtopic}}`, which the engine turns into a fixed reply. From that point the
   turn's tool handler refuses every further call (`{"error":"the answer is complete"}`), so
   nothing more is looked up or charged. The prompt file never
   contains the marker (T13's template renderer would read `{{offtopic}}` as a placeholder); the
   marker line is appended to `system[1]` in code.
5. **Tools.** Tools are scoped to the account server-side (the data source and the engine), and
   every input is validated before anything is read. The stored analysis cache (`get_eval`,
   read-only) is kept apart from interactive checks (`analyse_position`, charged to the
   engine-check quota).
6. **Engine pending.** An engine check still running after the 5 s inline wait streams one
   interim line ("Checking that position with the engine, one moment.", `interim: true`, at most
   once per turn) and waits up to 45 s in all, inside the tool call (under the tool endpoint's
   50 s per-call timeout). A check still pending then is abandoned (its quota refunded) and the
   model gets `engine busy`.
7. **History.** Our Postgres conversation is the record (ADR-R3-6 Amendment 1); the SDK
   session carries the model's view, and it may only be resumed while the two agree.
   - Every outcome carries `session_in_sync`. It is true only for an `Answered` turn with no
     repair, no max-turns follow-up, no retracted message and nothing replaced, so the stored
     paragraphs are exactly what the session recorded (the server's drop and limit notes aside,
     which are never in the session). Off-topic, refusal and fallback replies are not in sync:
     the session holds the model's text, the record the fixed copy.
   - The next turn resumes only when the previous answer was in sync and the history does not
     roll over (`resume_next_turn`). It then sends only the question, with the earlier
     messages, flattened, as the rebuild context.
   - Otherwise it sends the full Postgres history (`Verbatim` or `Rollover`) with
     `resume: false` and no previous session usage. The sidecar deletes the stale session rows
     and starts a clean session under the same id, and metering uses the turn's own
     `model_usage` (path `new`).
   - Every `replay_turns` (6) turns the history rolls over into a summary plus a plain
     transcript of the recent turns. **A rollover turn always starts a new session**, since a
     resumed session would still hold the full verbatim history.
   - Session entries are never edited, and no earlier assistant (or thinking) block is ever
     edited or replayed.
   - Tool-call indexes are unique across the conversation: a turn starts at the previous
     outcome's `next_call_index` (`ToolSession::starting_at`), which the caller stores with the
     conversation. A `tools.<k>` token copied from an earlier turn then fails closed instead of
     resolving to another result.
8. **Prompts.** The chat's prompts are registered T13 files (`CHAT_PROMPT_SET`,
   `chat@1`): `system[0]` is the shared `rules_block()` (`core/rules` + `core/tokens`,
   byte-identical to the report's, so one cache entry serves both); `system[1]` is
   `chat/system`, whose "Output in this chat" section asks for plain paragraphs, with
   positions cited by their own tokens and no JSON, schema or `citations` field; the repair
   renders `chat/repair` with T13's `- path: code: detail` lines, the path naming the
   paragraph (`chat.<n>`). The report's `report/output` and `repair/repair` are never sent in
   a chat.
9. **Follow-ups.** The round-cap follow-up ("Answer now with what you have.") and the repair run
   as one-round turns without tools under a fresh session id, with the flattened transcript and
   the tool results as data. They never enter the conversation's session; their session-store
   rows are swept as orphans (Task 21). Only the main turn's `Done` gives the stored
   `sessionModelUsage`. A turn with a follow-up or a repair is never in sync (§7), so the next
   turn rebuilds the session from our record rather than resuming the unrepaired one. The
   repair is not run inside the conversation's session: that session would then hold the
   invalid answer, the repair prompt and the repaired answer.
   A paragraph cut off by `max_tokens` is never shown; it holds the stream and is repaired.
   The follow-up never sees the main turn's streamed text, so its answer stands alone: the main
   turn's paragraphs are dropped (and the stream replaced) instead of being said twice.
10. **Quotas.** Free and Pro chat quotas come from `entitlements::chat_quotas` (free: 3 a week;
   Pro: 30 a day and 300 a month); a conversation holds at most 40 turns and a question at most
   2000 characters. Denials use fixed copy with no upsell.
11. **Web hand-off.** `chat.chunk` carries `seq`, the paragraph's token text, its rendered
    `segments` and `interim`; `chat.message.ready` carries the final segments and `replace`
    (the streamed chunks must be discarded: a repair, a refusal, an off-topic or fallback reply
    after something was streamed, or a retracted message).

## Consequences

- The player only ever sees validated paragraphs, at the cost of a held stream when a paragraph
  fails, and one extra call for the repair.
- The turn after a repair, follow-up, refusal, off-topic or fallback reply pays a full uncached
  prefix, because it starts a new session from our record. That is rare, and in exchange the
  model never sees a rejected answer as accepted history.
- Stricter rules than the brief's two (`engine_claims`, `no_quantity_words`) follow Task 8's fix
  rounds, which made them universal for the report: more chat paragraphs may be repaired or
  dropped, never fewer checked.

## Interface (Task 23)

`run_chat_turn(ai, before_call, deps: &OwnedToolDeps, args: ChatTurnArgs, on_chunk)` departs
from the plan's signature:
- `OwnedToolDeps` (T22's `Arc` handles) instead of `&ToolDeps`, because `stream_turn` takes an
  `Arc<dyn ToolHandler>`, which must be `'static`;
- `ChatTurnArgs { evidence, messages, conversation_id, resume,
  previous_session_usage, first_call_index }` instead of `input` and `messages`: the SDK
  session (id, resume), metering and the conversation-wide call indexes need these. `evidence`
  is `build_evidence(input)` plus the anchors through `anchor_details`; the caller must build it
  from the same input and anchors as `messages`.

## Hand-offs

- **Task 25** stores `session_in_sync` on the assistant row and `next_call_index` on the
  conversation, and uses `resume_next_turn`. A chat turn makes up to three sequential
  `stream_turn` calls (main, follow-up, repair), so the per-call `timeoutMs` must be at most
  `CHAT_TURN_TIMEOUT / 3`, or the reconcile sweep fails a live turn. Count and reserve the chat
  quota atomically (two concurrent questions can both pass the pure pre-check).
- **Task 24.** If a turn is cancelled while a tool call waits on the engine, the check may never
  be abandoned; the reconciler must expire stale pending checks and refund them.

## The coach-chat job (Task 25)

Decisions made when the job, the DB data source and the threads were built:

1. **One turn per conversation.** The job holds a session advisory lock (class 30003, the
   conversation id) on a dedicated connection that is closed, never pooled, when the handler
   ends; a dropped handler or a crashed worker frees it with the connection. A second handler
   (a redelivered job, or another answer of the thread) is released and retried. Under the lock,
   an answer waits while an earlier answer of the thread is unfinished, so turns run in `seq`
   order. `post_user_message` also refuses a question while an answer is pending or streaming
   (`Busy`, under a row lock on the conversation). The finish transaction runs on the lock
   connection, so a lost lock fails the write rather than interleaving it.
2. **The chat allowance is reserved atomically** (hand-off above). Under a transaction lock per
   account of the budget scope (class 30004), the job counts the scope's assistant answers per
   window and moves the message to `streaming` in the same transaction. Counted: `streaming`
   answers, and `done` answers that carry an AI disclosure. The fixed no-data copy has none, so
   it does not use the allowance. The reservation happens before the no-data and no-AI checks
   (the brief's order of the checks is kept).
3. **State that survives restarts** lives in Postgres (migration `20261121030200`):
   `coach_conversations.next_call_index` (the next turn's first tool-call index; raised with
   `greatest` in the finish transaction), `coach_messages.session_in_sync` and
   `coach_messages.turn_attempts`. The next turn resumes only when the previous assistant row is
   `done` with `session_in_sync = true` and the history did not roll over. A failed or lost turn
   is never in sync, so the turn after it sends the full Postgres history with `resume: false`
   and the sidecar replaces the partial session.
4. **Unavailable retries.** A `Release` gives its queue attempt back, so the job cannot count
   attempts from the queue. `turn_attempts` counts the turn's AI attempts: the first unavailable
   attempt puts the message back to `pending` (the retry reserves again) and releases for 60 s;
   the second fails with `unavailable`. The whole turn runs under `CHAT_TURN_TIMEOUT` (600 s); a
   timeout counts as unavailable.
5. **Timeouts.** `CHAT_CALL_TIMEOUT` = 200 s = `CHAT_TURN_TIMEOUT / 3` is the most one sidecar
   call may take; `SidecarConfig::chat_timeout` (180 s) is under it, which a test pins. This
   supersedes the earlier note to pass `CHAT_TURN_TIMEOUT` itself as `timeoutMs`: a turn makes up
   to three sequential calls.
6. **History after a rollover.** Once a summary exists, later turns (until the next rollover) use
   `Rollover { stored summary, the last replay_turns turns }` without a new summary call. Only a
   turn that writes a new summary counts as a rollover for `resume_next_turn`, so the session
   started by the rollover turn is resumed afterwards.
7. **Deleted mid-turn.** The finish transaction re-checks the conversation with `FOR SHARE`
   (waiting for a delete in progress). If it is gone, the paid calls are still metered (usage rows
   have no content) and the conversation's orphaned session entries are deleted.
8. **Titles.** After the first answered turn, `summarise_thread` (metered as chat) gives the
   title, or the anchor's fallback title when the call fails or the title does not pass.
   Refusal, off-topic and fallback turns get the fallback title without a call.
9. **The data source** reads weaknesses, openings and exemplars from the conversation's coaching
   input (the newest report-run snapshot of the account and time class), and positions, games,
   cached evals and report sections from Postgres, always filtered on the account. `get_games` is
   limited to the conversation's time class. `openingContains` matches literally (`\`, `%` and `_`
   escaped). Position tags come from the matching exemplars until R2's per-move tags exist.
10. **Threads take the account.** `post_user_message` takes the account id the caller acts for
    (it is not in the plan's signature): every thread call is scoped by it.
11. **Task 18 stand-ins.** T18 is not on this base. `jobs/budget.rs` holds minimal versions of
    `BudgetConfig`, `budget_guard` and `meter`, and `ChatDeps` carries a `PricingTable`. At merge,
    T18's file replaces them and the call sites in `chat/job.rs` are adapted.
