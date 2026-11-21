-- TEST FIXTURE ONLY. Task 17 owns the real migration in db/migrations/; this mirrors the agreed
-- shape so the sidecar store can be tested in isolation. R3 (ADR-R3-6): Agent SDK chat
-- transcripts, mirrored by the ai-sidecar SessionStore.
-- Entries are opaque SDK JSON; the sidecar never interprets them. Rust deletes a
-- conversation's rows (session_id = conversation id) in the same transaction as the conversation.

CREATE TABLE ai_sdk_session_entries (
  project_key text NOT NULL,
  session_id uuid NOT NULL,
  subpath text NOT NULL DEFAULT '',
  seq bigint NOT NULL,
  entry jsonb NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (project_key, session_id, subpath, seq)
);

-- Rust's per-conversation delete and the sidecar's whole-session lookups filter on session_id alone.
CREATE INDEX ai_sdk_session_entries_session_idx ON ai_sdk_session_entries (session_id);

-- The sidecar's dedicated role. Roles are cluster-wide, so creation tolerates an existing role
-- (and a concurrent creator, as when parallel test databases migrate at once). The password is not
-- set here: deployment sets it from the environment (ALTER ROLE ai_sidecar PASSWORD ...), and the
-- sidecar reads its connection string from AI_SESSION_DATABASE_URL.
DO $$
BEGIN
  CREATE ROLE ai_sidecar LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT;
EXCEPTION
  WHEN duplicate_object OR unique_violation THEN NULL;
END
$$;

-- This table only, and only what the SessionStore needs.
GRANT SELECT, INSERT, DELETE ON ai_sdk_session_entries TO ai_sidecar;
