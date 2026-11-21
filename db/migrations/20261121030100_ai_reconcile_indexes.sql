-- R3 Task 21 (ADR-R3-3): the ai-reconcile sweep of lost chat answers reads pending assistant
-- messages oldest first (jobs::reconcile::PENDING_CHATS_SQL); without this index it would scan
-- every message.
CREATE INDEX coach_messages_pending_idx ON coach_messages (created_at) WHERE status = 'pending';
