-- R3 Task 26 (ADR 0059, ADR 0064): chat threads belong to M4's "user" rows.
-- coach_conversations.user_id REFERENCES "user"(id) ON DELETE CASCADE: deleting a user deletes
-- their threads, their messages (the existing cascade) and their Agent SDK session entries (the
-- coach_conversations delete trigger of 20261121030000).
--
-- Ordering: M4's 20261121010000_web_auth sorts before this file, so on the integrated schema the
-- table exists and the constraint is added. R3's branch alone has no "user" table; there this
-- migration adds nothing (a database built from R3 alone is never deployed). It is idempotent.
-- user_events.user_id gets no FK: the TS hub tests insert owned events for synthetic users, and
-- an orphaned owned event is unreadable by anyone (the readers filter on the signed-in user).
DO $$
BEGIN
  IF to_regclass('public."user"') IS NOT NULL
     AND NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'coach_conversations_user_id_fk')
  THEN
    ALTER TABLE coach_conversations
      ADD CONSTRAINT coach_conversations_user_id_fk
      FOREIGN KEY (user_id) REFERENCES "user"(id) ON DELETE CASCADE;
  END IF;
END
$$;
