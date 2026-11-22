-- R2 Task 32 (coordinator rulings on concerns 4 and 5, ADR-R2-11).
--
-- 1. An opponent slope that cannot be estimated is unknown: NULL (no adjustment), never NaN,
--    an infinity or a made-up 0. Postgres treats 'NaN' = 'NaN' as true, so NOT IN refuses it.
ALTER TABLE band_reference ALTER COLUMN opp_slope_per_100 DROP NOT NULL;
ALTER TABLE band_reference ADD CONSTRAINT band_reference_slope_finite
  CHECK (opp_slope_per_100 IS NULL
         OR opp_slope_per_100 NOT IN ('NaN'::real, 'Infinity'::real, '-Infinity'::real));

-- 2. A reference game whose enrichment left plies pending because Maia was unavailable is
--    flagged with the first time that happened. The re-enrichment sweep re-runs flagged games
--    of building builds once Maia answers; finalisation waits for them up to a grace period.
ALTER TABLE reference_games ADD COLUMN difficulty_pending_at timestamptz;
CREATE INDEX reference_games_difficulty_pending_idx
  ON reference_games (difficulty_pending_at, id) WHERE difficulty_pending_at IS NOT NULL;
