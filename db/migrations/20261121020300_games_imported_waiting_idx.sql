-- R2 Task 29 (ADR-R2-16): the reconciler's straggler query reads only the `imported` games still
-- waiting for the scheduler (`pending_since` set; the schedule-analysis job clears it on every
-- game it has read). Partial, so its size is the number of stragglers, not of imported games.
CREATE INDEX games_imported_waiting_idx ON games (account_id)
  WHERE analysis_status = 'imported' AND pending_since IS NOT NULL;
