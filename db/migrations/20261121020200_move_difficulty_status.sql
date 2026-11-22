-- R2 Task 25 (ADR-R2-14): per-ply difficulty status, so `pending_maia` positions can be
-- re-enriched once Maia is healthy (owner override: difficulty comes only from Maia).
--
-- difficulty_status: 'ok' (p_best/p_ok/difficulty_model set), 'pending_maia' (Maia missing,
--   down or answering invalidly; retried by the sweep) or 'not_applicable' (final). NULL: no
--   difficulty was requested (decided ply, or a row from before this migration).
-- difficulty_reason: the PendingReason / NotApplicableReason in snake_case; NULL when ok.
-- difficulty_attempts: invalid answers from a healthy Maia. At MAX_DIFFICULTY_ATTEMPTS (3) the
--   row becomes not_applicable/model_error, so a deterministic model error is not retried forever.
ALTER TABLE move_features ADD COLUMN difficulty_status text;
ALTER TABLE move_features ADD COLUMN difficulty_reason text;
ALTER TABLE move_features ADD COLUMN difficulty_attempts smallint NOT NULL DEFAULT 0;
ALTER TABLE move_features ADD CONSTRAINT move_features_difficulty_status_check
  CHECK (difficulty_status IS NULL OR difficulty_status IN ('ok', 'pending_maia', 'not_applicable'))
  NOT VALID;
ALTER TABLE move_features VALIDATE CONSTRAINT move_features_difficulty_status_check;
-- The re-enrichment sweep pages through pending rows by (game_id, ply).
CREATE INDEX move_features_pending_maia_idx ON move_features (game_id, ply)
  WHERE difficulty_status = 'pending_maia';
