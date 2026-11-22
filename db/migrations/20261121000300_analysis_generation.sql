-- ADR 0035: analysis generations. A re-analysis request bumps analysis_generation (and resets
-- analysis_attempts); a game is "done" only while analysed_generation = analysis_generation.
ALTER TABLE games ADD COLUMN analysis_generation integer NOT NULL DEFAULT 0;
ALTER TABLE games ADD COLUMN analysed_generation integer;
UPDATE games SET analysed_generation = 0 WHERE analysis_status = 'done';
CREATE INDEX games_attempt_cap_idx ON games (analysis_status) WHERE analysis_attempts >= 6;
CREATE INDEX import_runs_paused_idx ON import_runs (account_id) WHERE status = 'paused';
