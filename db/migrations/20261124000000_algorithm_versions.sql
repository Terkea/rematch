-- ADRs 0104 and 0105: the algorithm that produced each recommendation. `algorithm_versions` holds
-- every manifest a worker or the web has written with (`algorithm/manifest.json`, inserted on
-- first use with ON CONFLICT DO NOTHING). Each table a recommendation comes from gets a nullable
-- `algorithm_id`; NULL means "before versioning" and is never backfilled. `position_evals` gets
-- no column: its key is (fen_key, protocol), and the protocol is the manifest's engine protocol,
-- so the eval cache is shared by every version with the same engine protocol.
CREATE TABLE algorithm_versions (
  id text PRIMARY KEY,
  manifest jsonb NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE coaching_inputs ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
ALTER TABLE report_runs ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
ALTER TABLE weekly_reviews ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
ALTER TABLE coach_messages ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
ALTER TABLE focuses ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
ALTER TABLE drill_items ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
ALTER TABLE kpi_snapshot ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
ALTER TABLE weakness_estimates ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
ALTER TABLE weakness_state_history ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
ALTER TABLE game_metrics ADD COLUMN algorithm_id text REFERENCES algorithm_versions (id);
