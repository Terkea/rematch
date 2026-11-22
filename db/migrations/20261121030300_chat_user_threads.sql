-- R3 Task 25 fix round 1 (review I1, I3, m6).

-- Threads belong to the user who started them: linked accounts are shared between users (M4
-- account_links). No beta threads exist yet, so there is nothing to backfill; a NULL row makes this
-- migration fail loudly rather than guess an owner.
ALTER TABLE coach_conversations ALTER COLUMN user_id SET NOT NULL;
DROP INDEX coach_conversations_user_idx;
CREATE INDEX coach_conversations_user_idx
  ON coach_conversations (user_id, account_id, last_message_at DESC);

-- The FK to M4's "user" table. r3/base has no web auth tables, so it is added only where the table
-- exists (the integrated schema, whose M4 migrations sort before this one). Task 26 checks it.
DO $$
BEGIN
  IF to_regclass('public."user"') IS NOT NULL THEN
    ALTER TABLE coach_conversations ADD CONSTRAINT coach_conversations_user_id_user_id_fk
      FOREIGN KEY (user_id) REFERENCES "user"(id) ON DELETE CASCADE;
  END IF;
END
$$;

-- Idempotent metering: one row per paid call. The coach chat keys a call by message, attempt and
-- call number, so a retried or re-run write never meters a call twice.
ALTER TABLE ai_usage ADD COLUMN idempotency_key text;
CREATE UNIQUE INDEX ai_usage_idempotency_uq ON ai_usage (idempotency_key)
  WHERE idempotency_key IS NOT NULL;

ALTER TABLE coach_conversations ADD CONSTRAINT coach_conversations_next_call_index_check
  CHECK (next_call_index >= 0);
ALTER TABLE coach_messages ADD CONSTRAINT coach_messages_turn_attempts_check
  CHECK (turn_attempts >= 0);
