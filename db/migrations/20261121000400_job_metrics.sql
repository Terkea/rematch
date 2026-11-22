-- ADR 0036: one row of per-stage timings per finished job attempt (all durations in ms).
CREATE TABLE job_metrics (
  id bigserial PRIMARY KEY, job_id bigint, kind text NOT NULL, game_id uuid, account_id uuid,
  queue_wait_ms double precision, parse_ms double precision, cache_ms double precision,
  engine_wall_ms double precision, engine_cpu_ms double precision, persist_ms double precision,
  total_ms double precision NOT NULL, positions integer, engine_positions integer,
  cache_hits integer, lichess_used integer, created_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX job_metrics_created_idx ON job_metrics (created_at);
