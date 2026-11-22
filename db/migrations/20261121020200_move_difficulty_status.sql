-- R2 Task 25 (ADR-R2-14): per-ply difficulty status, so `pending_maia` positions can be
-- re-enriched once Maia is healthy (owner override: difficulty comes only from Maia).
--
-- difficulty_status: 'ok' (p_best/p_ok/difficulty_model set), 'pending_maia' (Maia missing,
--   down or answering invalidly; retried by the sweep) or 'not_applicable' (final). NULL: no
--   difficulty was requested (decided ply, or a row from before this migration).
-- difficulty_reason: the PendingReason / NotApplicableReason in snake_case; NULL when ok.
-- difficulty_attempts: item-level invalid answers about this position from a healthy Maia. At
--   MAX_DIFFICULTY_ATTEMPTS (3) the row becomes not_applicable/model_error, so a deterministic
--   model error is not retried forever. Call-level failures never count here.
-- difficulty_call_failures: failed calls (transport, timeout, an unusable whole answer) charged
--   while Maia stayed healthy. The sweep orders by it, so such rows never block the others, and
--   skips rows at MAX_CALL_FAILURES (5); they stay pending_maia.
--
-- Locking: sqlx runs this file in one transaction, so the ADD COLUMNs (metadata-only), the
-- CHECKs (a scan) and the index build hold an ACCESS EXCLUSIVE lock on move_features until
-- commit. Fine at beta size; on a large table split it into a `-- no-transaction` migration with
-- `NOT VALID` + `VALIDATE` and `CREATE INDEX CONCURRENTLY`.
ALTER TABLE move_features ADD COLUMN difficulty_status text;
ALTER TABLE move_features ADD COLUMN difficulty_reason text;
ALTER TABLE move_features ADD COLUMN difficulty_attempts smallint NOT NULL DEFAULT 0;
ALTER TABLE move_features ADD COLUMN difficulty_call_failures smallint NOT NULL DEFAULT 0;
ALTER TABLE move_features ADD CONSTRAINT move_features_difficulty_status_check
  CHECK (difficulty_status IS NULL OR difficulty_status IN ('ok', 'pending_maia', 'not_applicable'));
-- With a status: an ok row has all of Maia's values, any other row has none of them.
ALTER TABLE move_features ADD CONSTRAINT move_features_difficulty_ok_check
  CHECK (difficulty_status IS NULL
         OR (difficulty_status = 'ok')
            = (p_best IS NOT NULL AND p_ok IS NOT NULL AND difficulty_model IS NOT NULL));
-- The re-enrichment sweep pages through pending rows by (call failures, game_id, ply).
CREATE INDEX move_features_pending_maia_idx
  ON move_features (difficulty_call_failures, game_id, ply)
  WHERE difficulty_status = 'pending_maia';
