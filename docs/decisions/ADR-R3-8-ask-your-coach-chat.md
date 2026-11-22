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
   exactly `{{offtopic}}`, which the engine turns into a fixed reply. The prompt file never
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
7. **History.** Our Postgres conversation is the record; the SDK session carries the model's
   view. History is append-only: a new SDK session (turn 1, or after a rollover) gets the full
   messages; later turns resume and send only the question, with the earlier messages,
   flattened, as the rebuild context. Every `replay_turns` (6) turns the history rolls over into
   a summary plus a plain transcript of the recent turns, in a fresh session, so no earlier
   assistant (or thinking) block is ever edited or replayed.
8. **Follow-ups.** The round-cap follow-up ("Answer now with what you have.") and the repair run
   as one-round turns without tools under a fresh session id, with the flattened transcript and
   the tool results as data. They never enter the conversation's session; their session-store
   rows are swept as orphans (Task 21). Only the main turn's `Done` gives the stored
   `sessionModelUsage`.
9. **Quotas.** Free and Pro chat quotas come from `entitlements::chat_quotas` (free: 3 a week;
   Pro: 30 a day and 300 a month); a conversation holds at most 40 turns and a question at most
   2000 characters. Denials use fixed copy with no upsell.
10. **Web hand-off.** `chat.chunk` carries `seq`, the paragraph's token text, its rendered
    `segments` and `interim`; `chat.message.ready` carries the final segments and `replace`
    (the streamed chunks must be discarded: a repair, a refusal, an off-topic or fallback reply
    after something was streamed, or a retracted message).

## Consequences

- The player only ever sees validated paragraphs, at the cost of a held stream when a paragraph
  fails, and one extra call for the repair.
- The SDK session keeps the model's original (unrepaired) answer; the next turn's model view
  can differ from what the player saw. The rebuild path and rollovers use our record.
- Stricter rules than the brief's two (`engine_claims`, `no_quantity_words`) follow Task 8's fix
  rounds, which made them universal for the report: more chat paragraphs may be repaired or
  dropped, never fewer checked.
