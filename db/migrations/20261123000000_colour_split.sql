-- The colour split (next round T1): profile statistics per side the player had.
--
-- `side` is 'both' (every game, what the profile always showed), 'white' or 'black' (only the
-- games the player had that colour in). Existing rows are 'both'. Per-side rows obey the same
-- minimum-evidence rules, counted per side. The weakness timeline
-- (`weakness_state_history`) stays both-only: per-colour states would halve the evidence.
-- A both band reference is never served for one colour: no per-side row means no benchmark.
CREATE TYPE side_scope AS ENUM ('both', 'white', 'black');

ALTER TABLE kpi_snapshot ADD COLUMN side side_scope NOT NULL DEFAULT 'both';
DROP INDEX kpi_snapshot_lookup_idx;
CREATE INDEX kpi_snapshot_lookup_idx
  ON kpi_snapshot (account_id, time_class, side, metric, window_key, computed_at DESC);

ALTER TABLE weakness_estimates ADD COLUMN side side_scope NOT NULL DEFAULT 'both';
ALTER TABLE weakness_estimates DROP CONSTRAINT weakness_estimates_pkey;
ALTER TABLE weakness_estimates ADD PRIMARY KEY (account_id, time_class, side, series_key);

ALTER TABLE band_reference ADD COLUMN side side_scope NOT NULL DEFAULT 'both';
ALTER TABLE band_reference DROP CONSTRAINT band_reference_pkey;
ALTER TABLE band_reference ADD PRIMARY KEY (build_id, band, time_class, side, metric);
