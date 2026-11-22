-- R3 Task 21 (ADR-R3-3 amendment 1): what the ai-reconcile sweep needs.

-- The crash cap: each reconciler re-enqueue bumps the row's count first; past the cap
-- (jobs::reconcile::RECONCILE_CAP) the row is failed instead of enqueued again.
ALTER TABLE report_runs ADD COLUMN reconcile_count smallint NOT NULL DEFAULT 0;
ALTER TABLE weekly_reviews ADD COLUMN reconcile_count smallint NOT NULL DEFAULT 0;
ALTER TABLE weakness_change_notes ADD COLUMN reconcile_count smallint NOT NULL DEFAULT 0;

-- When the assistant turn started streaming (db::chat::set_streaming); a turn streaming for
-- longer than the turn timeout with no live job is lost.
ALTER TABLE coach_messages ADD COLUMN streaming_at timestamptz;

-- The sweep pages of lost chat answers (jobs::reconcile::PENDING_CHATS_SQL, STREAMING_CHATS_SQL),
-- each read in order of its age column.
CREATE INDEX coach_messages_pending_idx ON coach_messages (created_at) WHERE status = 'pending';
CREATE INDEX coach_messages_streaming_idx ON coach_messages (streaming_at)
  WHERE status = 'streaming';
