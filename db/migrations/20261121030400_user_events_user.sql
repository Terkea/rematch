-- R3 Task 25 fix round 2 (review N1): user-owned events.
--
-- Rule: an event with a non-null user_id is for that user only. Chat events (chat.chunk with
-- answer text, chat.message.ready, chat.message.failed) are written with the thread owner's user,
-- because a linked account is shared between users. Every reader (the web SSE hub, the poll
-- route) MUST filter `user_id IS NULL OR user_id = <the signed-in user>` (ADR-R3-8 §16). Account
-- events keep user_id NULL and reach every user of the account, as before.
ALTER TABLE user_events ADD COLUMN user_id uuid;
CREATE INDEX user_events_user_idx ON user_events (user_id, id) WHERE user_id IS NOT NULL;
