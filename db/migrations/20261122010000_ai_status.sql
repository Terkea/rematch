-- Bug hunt I5 (ADR 0078): whether a worker's AI runners can write coach text. Each worker with AI
-- runners upserts its row at start (`enabled` false when it has no AI client: no sidecar, no
-- credential, a failed health check) and refreshes `heartbeat_at` while it runs. The web reads
-- it to say "Coach not available right now" instead of offering an answer nothing will write: the
-- coach counts as available only while some row is enabled with a fresh heartbeat. A stopped
-- worker's row just goes stale; nothing deletes it.
CREATE TABLE ai_status (
  worker_id uuid PRIMARY KEY,
  enabled boolean NOT NULL,
  -- The worker's `ai: enabled (...)` / `ai: disabled (...)` log line; never a key or token.
  detail text NOT NULL,
  started_at timestamptz NOT NULL DEFAULT now(),
  heartbeat_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ai_status_heartbeat_idx ON ai_status (heartbeat_at);
