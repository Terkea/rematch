-- R3 Task 26 fix round 1 (ADR 0059, review I1): where each ai-reconcile sweep that pages through
-- a time window stopped, so the next sweep resumes there instead of at the window start. One row
-- per sweep (today only 'orphan_session_entries'). Losing a row only costs one rescan.
CREATE TABLE ai_sweep_cursors (
  name text PRIMARY KEY,
  position timestamptz NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now()
);
