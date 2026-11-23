-- ADR 0094: the asking user's plan context (their weekly focus, training and "Is it working?"
-- verdicts) that a chat answer was written from, kept with the answer as its coaching input is
-- (ADR 0086 §4). Per user by construction: a conversation belongs to one user.
ALTER TABLE coach_messages ADD COLUMN plan_context jsonb;
