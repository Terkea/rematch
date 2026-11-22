-- R3 Task 25 fix round 1 (review I1, I3, m6).

-- Threads belong to the user who started them: linked accounts are shared between users (M4
-- account_links). No beta threads exist yet, so there is nothing to backfill; a NULL row makes this
-- migration fail loudly rather than guess an owner.
ALTER TABLE coach_conversations ALTER COLUMN user_id SET NOT NULL;
DROP INDEX coach_conversations_user_idx;
CREATE INDEX coach_conversations_user_idx
  ON coach_conversations (user_id, account_id, last_message_at DESC);

-- The FK to M4's "user" table is added by Task 26's integration migration (r3/base has no web
-- auth tables). Tests create their users with testkit::insert_user, so they hold under it.

-- Idempotent metering: one row per paid call. The coach chat keys a call by message, attempt and
-- call number, so a retried or re-run write never meters a call twice.
-- T18's 20261121031800 adds the same column and index, so both are IF NOT EXISTS (either order).
ALTER TABLE ai_usage ADD COLUMN IF NOT EXISTS idempotency_key text;
CREATE UNIQUE INDEX IF NOT EXISTS ai_usage_idempotency_uq ON ai_usage (idempotency_key)
  WHERE idempotency_key IS NOT NULL;

ALTER TABLE coach_conversations ADD CONSTRAINT coach_conversations_next_call_index_check
  CHECK (next_call_index >= 0);
ALTER TABLE coach_messages ADD CONSTRAINT coach_messages_turn_attempts_check
  CHECK (turn_attempts >= 0);
