-- R3 AI coaching (ADR-R3-3). R3 owns migration versions 20261121030000-20261121039999.
-- Enums are bound as text with casts by the Rust repositories (backend/crates/ai/src/db).

CREATE TYPE report_trigger AS ENUM ('onboarding','manual','scheduled','profile_change');
CREATE TYPE ai_plan AS ENUM ('free','pro');
CREATE TYPE report_mode AS ENUM ('sync','batch');
CREATE TYPE report_run_status AS ENUM ('queued','running','awaiting_retry','awaiting_batch','done','failed');
CREATE TYPE report_status AS ENUM ('computed_only','partial','complete');
CREATE TYPE ai_item_status AS ENUM ('pending','ready','dropped','failed');
CREATE TYPE chat_role AS ENUM ('user','assistant');
CREATE TYPE chat_status AS ENUM ('pending','streaming','done','failed');
CREATE TYPE ai_feature AS ENUM ('report','weekly_review','weakness_change','chat');
CREATE TYPE ai_batch_status AS ENUM ('submitted','ended','processed','failed');

CREATE TABLE report_runs (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  time_class time_class NOT NULL, trigger report_trigger NOT NULL, plan ai_plan NOT NULL,
  mode report_mode NOT NULL DEFAULT 'sync', status report_run_status NOT NULL DEFAULT 'queued',
  retry_count integer NOT NULL DEFAULT 0, last_error text, next_attempt_at timestamptz,
  started_at timestamptz, heartbeat_at timestamptz, finished_at timestamptz,
  prompt_version text NOT NULL, prompt_hash text, input_hash text, input jsonb,
  is_first_report boolean NOT NULL, report_id uuid, ai_disabled_reason text,
  created_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX report_runs_account_idx ON report_runs (account_id, time_class, created_at DESC);
CREATE INDEX report_runs_status_idx ON report_runs (status, next_attempt_at);
CREATE TABLE reports (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  time_class time_class NOT NULL, version integer NOT NULL,
  report_run_id uuid NOT NULL UNIQUE REFERENCES report_runs(id),
  schema_version text NOT NULL, computed jsonb NOT NULL, status report_status NOT NULL,
  ai_disclosure jsonb, created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (account_id, time_class, version));
CREATE TABLE report_sections (
  report_id uuid NOT NULL REFERENCES reports(id) ON DELETE CASCADE,
  section text NOT NULL CHECK (section IN ('overview','weaknesses','openings','climb_plan','areas')),
  status ai_item_status NOT NULL CHECK (status IN ('ready','dropped')),
  content jsonb, dropped jsonb NOT NULL DEFAULT '[]', attempts smallint NOT NULL, model text NOT NULL,
  validated_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY (report_id, section));
CREATE TABLE weekly_reviews (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  time_class time_class NOT NULL, week_start date NOT NULL, status ai_item_status NOT NULL DEFAULT 'pending',
  mode report_mode NOT NULL DEFAULT 'sync', input jsonb NOT NULL, content jsonb, dropped jsonb NOT NULL DEFAULT '[]',
  model text, prompt_hash text, retry_count integer NOT NULL DEFAULT 0, next_attempt_at timestamptz,
  ai_disclosure jsonb, created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (account_id, time_class, week_start));
CREATE TABLE weakness_change_notes (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  change_key text NOT NULL, series_key text NOT NULL, onset_date date,
  status ai_item_status NOT NULL DEFAULT 'pending', input jsonb NOT NULL, text text, segments jsonb,
  citations jsonb NOT NULL DEFAULT '[]', model text, retry_count integer NOT NULL DEFAULT 0,
  next_attempt_at timestamptz, ai_disclosure jsonb,
  created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (account_id, change_key));
CREATE TABLE coach_conversations (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  user_id uuid,                          -- M4's user; the FK is added by M4's Better Auth migration
  time_class time_class NOT NULL, title text, anchor jsonb,
  summary text, summary_through_seq integer NOT NULL DEFAULT 0,
  created_at timestamptz NOT NULL DEFAULT now(), last_message_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX coach_conversations_owner_idx ON coach_conversations (account_id, last_message_at DESC);
CREATE TABLE coach_messages (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  conversation_id uuid NOT NULL REFERENCES coach_conversations(id) ON DELETE CASCADE,
  seq integer NOT NULL, role chat_role NOT NULL, status chat_status NOT NULL, anchor jsonb,
  text text, segments jsonb, citations jsonb NOT NULL DEFAULT '[]', api_messages jsonb,
  dropped_count integer NOT NULL DEFAULT 0, failure_reason text, ai_disclosure jsonb,
  -- How the Agent SDK session was used for this assistant turn (metrics; ADR-R3-6 amendment 1).
  session text CHECK (session IN ('new','resumed','rebuilt')),
  created_at timestamptz NOT NULL DEFAULT now(), UNIQUE (conversation_id, seq));
CREATE TYPE interactive_eval_status AS ENUM ('pending','done','failed');
-- Interactive engine checks: NEVER read by KPI, stats, profile or report code (Global Constraints).
CREATE TABLE interactive_evals (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  message_id uuid REFERENCES coach_messages(id) ON DELETE SET NULL,
  source text NOT NULL DEFAULT 'interactive' CHECK (source = 'interactive'),
  protocol text NOT NULL, base_fen text NOT NULL, moves text[] NOT NULL DEFAULT '{}', fen text NOT NULL,
  status interactive_eval_status NOT NULL DEFAULT 'pending', lines jsonb, error text,
  charged boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now(), finished_at timestamptz);
CREATE INDEX interactive_evals_quota_idx ON interactive_evals (account_id, created_at) WHERE charged;
CREATE INDEX interactive_evals_reuse_idx ON interactive_evals (account_id, fen, protocol, created_at DESC);
CREATE TABLE ai_usage (
  id bigserial PRIMARY KEY, account_id uuid NOT NULL, feature ai_feature NOT NULL, model text NOT NULL,
  batch boolean NOT NULL DEFAULT false, input_tokens integer NOT NULL, output_tokens integer NOT NULL,
  cache_creation_tokens integer NOT NULL, cache_read_tokens integer NOT NULL, cost_micro_usd bigint NOT NULL,
  ref_id text, created_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX ai_usage_account_idx ON ai_usage (account_id, created_at);
CREATE INDEX ai_usage_created_idx ON ai_usage (created_at);
CREATE TABLE ai_batches (
  id text PRIMARY KEY, kind text NOT NULL CHECK (kind IN ('report','weekly_review')),
  status ai_batch_status NOT NULL DEFAULT 'submitted', request_count integer NOT NULL,
  submitted_at timestamptz NOT NULL DEFAULT now(), ended_at timestamptz, processed_at timestamptz, last_polled_at timestamptz);
CREATE TABLE ai_batch_items (
  id uuid PRIMARY KEY, batch_id text NOT NULL REFERENCES ai_batches(id) ON DELETE CASCADE,
  custom_id text NOT NULL UNIQUE, kind text NOT NULL CHECK (kind IN ('report','weekly_review')),
  report_run_id uuid REFERENCES report_runs(id) ON DELETE CASCADE, section text,
  weekly_review_id uuid REFERENCES weekly_reviews(id) ON DELETE CASCADE, processed_at timestamptz);
CREATE INDEX ai_batch_items_pending_idx ON ai_batch_items (batch_id) WHERE processed_at IS NULL;

-- Claude Agent SDK session store (ADR-R3-6 amendment 1). The sidecar's Postgres SessionStore
-- adapter writes opaque transcript entries here; session_id is the coach_conversations id. The
-- SDK never deletes: Rust deletes a conversation's rows in the same transaction as the
-- conversation (db::chat::delete_conversation, delete_all_conversations). No FK, so the sidecar
-- may write a turn's entries before Rust commits anything about it.
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
