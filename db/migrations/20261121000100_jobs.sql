-- Postgres job queue (ADR 0032): replaces Redis/BullMQ. Claimed with FOR UPDATE SKIP LOCKED.
CREATE TYPE job_state AS ENUM ('queued', 'running', 'done', 'failed', 'dead', 'paused');
CREATE TABLE jobs (
  id bigserial PRIMARY KEY,
  kind text NOT NULL,
  payload jsonb NOT NULL,
  dedupe_key text,
  user_key uuid,                      -- account id: the fair-scheduling hook for R2 (§8c/§8d)
  priority smallint NOT NULL DEFAULT 100,   -- lower runs first
  run_at timestamptz NOT NULL DEFAULT now(),
  created_at timestamptz NOT NULL DEFAULT now(),
  state job_state NOT NULL DEFAULT 'queued',
  attempts integer NOT NULL DEFAULT 0,
  max_attempts integer NOT NULL DEFAULT 3,
  stalls integer NOT NULL DEFAULT 0,
  max_stalls integer NOT NULL DEFAULT 2,
  recur_every interval,
  lease_owner text,
  lease_token uuid,
  lease_until timestamptz,
  heartbeat_at timestamptz,
  started_at timestamptz,
  finished_at timestamptz,
  last_error text,
  error_code text
);
CREATE UNIQUE INDEX jobs_dedupe_live_uq ON jobs (kind, dedupe_key)
  WHERE dedupe_key IS NOT NULL AND state IN ('queued', 'running', 'paused');
CREATE INDEX jobs_ready_idx ON jobs (kind, priority, run_at, id) WHERE state = 'queued';
CREATE INDEX jobs_user_ready_idx ON jobs (kind, user_key, priority, run_at) WHERE state = 'queued';
CREATE INDEX jobs_lease_idx ON jobs (lease_until) WHERE state = 'running';
CREATE INDEX jobs_finished_idx ON jobs (state, finished_at) WHERE state IN ('done', 'failed', 'dead');
