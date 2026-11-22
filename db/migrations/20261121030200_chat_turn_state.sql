-- R3 Task 25: per-turn chat state that must survive restarts (ADR-R3-8 §7, T23 rulings I1/I2).

-- The first tool-call index of the conversation's next turn (ToolSession::starting_at), so a
-- `tools.<k>` token of an earlier turn never resolves to a different result.
ALTER TABLE coach_conversations ADD COLUMN next_call_index integer NOT NULL DEFAULT 0;

-- Whether the stored answer is exactly what the conversation's Agent SDK session recorded
-- (ChatTurnOutcome::session_in_sync). NULL for a turn that never finished; the next turn resumes
-- the SDK session only when the previous assistant turn has true here.
ALTER TABLE coach_messages ADD COLUMN session_in_sync boolean;

-- AI attempts of an assistant turn (the coach-chat job releases an unavailable first attempt,
-- which does not use a queue attempt, so the count is kept here).
ALTER TABLE coach_messages ADD COLUMN turn_attempts smallint NOT NULL DEFAULT 0;
