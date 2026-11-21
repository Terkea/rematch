-- TEST FIXTURE ONLY: a copy of the ai_sdk_session_entries part of Task 17's migration
-- (r3/e db/migrations/20261121030000_ai_coaching.sql), which owns the real schema.

CREATE TABLE ai_sdk_session_entries (
  project_key text NOT NULL,
  session_id uuid NOT NULL,
  subpath text NOT NULL DEFAULT '',
  seq bigint NOT NULL,
  entry jsonb NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (project_key, session_id, subpath, seq));
CREATE INDEX ai_sdk_session_entries_session_idx ON ai_sdk_session_entries (session_id);
-- One row per SDK entry uuid within a (project, session, subpath): the sidecar appends with
-- INSERT ... ON CONFLICT DO NOTHING instead of scanning the session for duplicates.
CREATE UNIQUE INDEX ai_sdk_session_entries_uuid_uq
  ON ai_sdk_session_entries (project_key, session_id, subpath, (entry->>'uuid')) WHERE entry ? 'uuid';

-- The sidecar's restricted role: this one table and nothing else. Created NOLOGIN; operations
-- grant LOGIN and a password out of band (no secret in a migration). Roles are cluster-wide, so
-- creation is idempotent and tolerates concurrent migrations (parallel test databases). Where the
-- migrating user may not create roles, the role is left to operations and the grant is skipped.
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'ai_sidecar')
     AND EXISTS (SELECT 1 FROM pg_roles WHERE rolname = current_user AND (rolsuper OR rolcreaterole)) THEN
    BEGIN
      CREATE ROLE ai_sidecar NOLOGIN;
    EXCEPTION WHEN duplicate_object OR unique_violation THEN
      NULL;
    END;
  END IF;
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'ai_sidecar') THEN
    GRANT SELECT, INSERT, DELETE ON ai_sdk_session_entries TO ai_sidecar;
  ELSE
    RAISE NOTICE 'role ai_sidecar not created (no CREATEROLE); grant it access to ai_sdk_session_entries manually';
  END IF;
END
$$;
