-- R2 profile engine (R2 Task 3): TS M2 Task 2's schema in snake_case, plus the R2 additions
-- (`build_status.unverified`, `k_unweighted`, `source_url`, `games_scanned`, `game_previews`,
-- `job_metrics.stages` and the scheduler/explorer indexes). The only R2 migration.
-- Requires R1 T19 (`job_metrics`), which is older on every database.

-- `imported` = imported but not selected for analysis. Never used as a default in this file:
-- Postgres forbids using an enum value added in the same transaction.
ALTER TYPE analysis_status ADD VALUE 'imported';

CREATE TYPE band AS ENUM ('B0', 'B1', 'B2', 'B3', 'B4', 'B5');
CREATE TYPE weakness_state AS ENUM
  ('not_an_issue', 'emerging', 'active', 'improving', 'resolved', 'relapsed');
CREATE TYPE state_row_kind AS ENUM ('transition', 'onset');
CREATE TYPE kpi_window AS ENUM ('last_n', 'week');
CREATE TYPE reference_kind AS ENUM ('band_reference', 'opening_explorer');
CREATE TYPE build_status AS ENUM ('building', 'active', 'superseded', 'failed', 'unverified');
CREATE TYPE opening_verdict AS ENUM ('keep', 'fix', 'learn', 'replace');
CREATE TYPE line_kind AS ENUM ('chosen', 'faced');

-- games / linked_accounts -------------------------------------------------------------------
ALTER TABLE games ADD COLUMN ply_count integer;
ALTER TABLE games ADD COLUMN analysis_tier smallint;          -- 0..3 (scheduler tier)
ALTER TABLE games ADD COLUMN analysis_priority smallint;
ALTER TABLE games ADD COLUMN selected_at timestamptz;
ALTER TABLE games ADD COLUMN analysed_at timestamptz;
ALTER TABLE games ADD COLUMN enrichment_protocol text;
ALTER TABLE games ADD CONSTRAINT games_analysis_tier_range
  CHECK (analysis_tier IS NULL OR analysis_tier BETWEEN 0 AND 3) NOT VALID;
ALTER TABLE games VALIDATE CONSTRAINT games_analysis_tier_range;
ALTER TABLE linked_accounts ADD COLUMN analysis_scheduled_at timestamptz;

-- Deleting an account deletes its games (the baseline FK was `no action`, which blocked it);
-- every per-game table cascades from `games`, every account-scoped one from `linked_accounts`.
ALTER TABLE games DROP CONSTRAINT games_account_id_linked_accounts_id_fk;
ALTER TABLE games ADD CONSTRAINT games_account_id_linked_accounts_id_fk
  FOREIGN KEY (account_id) REFERENCES linked_accounts (id) ON DELETE CASCADE NOT VALID;
ALTER TABLE games VALIDATE CONSTRAINT games_account_id_linked_accounts_id_fk;

-- Fair scheduling (R-12). The claim query's `served` CTE counts every account's games started in
-- the last hour: a short range scan on the time, index-only through `account_id`.
CREATE INDEX games_started_account_idx ON games (analysis_started_at, account_id)
  WHERE analysis_started_at IS NOT NULL;
-- The selection scan: an account's games in one status and time class.
CREATE INDEX games_account_status_tc_idx ON games (account_id, analysis_status, time_class);

-- Per-move enrichment ----------------------------------------------------------------------
CREATE TABLE move_features (
  game_id uuid NOT NULL REFERENCES games (id) ON DELETE CASCADE,
  ply integer NOT NULL,
  -- raw
  multipv jsonb,
  best_pv text[] NOT NULL DEFAULT '{}'::text[],
  refutation_pv text[] NOT NULL DEFAULT '{}'::text[],
  threat jsonb,
  tb_wdl_before smallint,
  tb_wdl_after smallint,
  p_best real,
  p_ok real,
  difficulty_model text,
  -- derived
  is_critical boolean,
  multipv_gap real,
  in_top2 boolean,
  best_line jsonb,
  refutation_line jsonb,
  threat_line jsonb,
  threat_existed boolean,
  threat_addressed boolean,
  enrichment_protocol text NOT NULL,
  PRIMARY KEY (game_id, ply)
);

CREATE TABLE position_multipv (
  fen_key text NOT NULL,
  protocol text NOT NULL,
  lines jsonb NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (fen_key, protocol)
);

CREATE TABLE position_threats (
  fen_key text NOT NULL,
  protocol text NOT NULL,
  score_cp integer,
  score_mate integer,
  pv text[] NOT NULL DEFAULT '{}'::text[],
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (fen_key, protocol)
);

CREATE TABLE position_tb (
  fen_key text PRIMARY KEY,
  wdl smallint NOT NULL,
  source text NOT NULL DEFAULT 'syzygy5',
  created_at timestamptz NOT NULL DEFAULT now()
);

-- Per-game metrics and weakness series ------------------------------------------------------
-- `metrics` is a serialised `PlayerGameMetrics`; readers must treat `metrics->>'version'` other
-- than PLAYER_GAME_METRICS_VERSION as stale (global constraints, version gate).
-- `metrics_version` mirrors that field (a missing version reads as 0) so stale rows are cheap to
-- find: `WHERE account_id = $1 AND metrics_version <> $current`.
CREATE TABLE game_metrics (
  game_id uuid PRIMARY KEY REFERENCES games (id) ON DELETE CASCADE,
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  played_at timestamptz NOT NULL,
  protocol text NOT NULL,
  enrichment_protocol text,
  difficulty_model text,
  metrics jsonb NOT NULL,
  metrics_version integer GENERATED ALWAYS AS (COALESCE((metrics->>'version')::int, 0)) STORED,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX game_metrics_account_tc_played_idx
  ON game_metrics (account_id, time_class, played_at);
-- An account's stale rows (`account_id = $1 AND metrics_version <> $2`): an index-only scan.
CREATE INDEX game_metrics_version_idx
  ON game_metrics (account_id, metrics_version) INCLUDE (game_id);

CREATE TABLE weakness_series_point (
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  series_key text NOT NULL,
  game_id uuid NOT NULL REFERENCES games (id) ON DELETE CASCADE,
  played_at timestamptz NOT NULL,
  events integer NOT NULL,
  opportunities integer NOT NULL,
  epl real NOT NULL,
  protocol text NOT NULL,
  PRIMARY KEY (account_id, time_class, series_key, game_id)
);
CREATE INDEX weakness_series_point_series_played_idx
  ON weakness_series_point (account_id, time_class, series_key, played_at);
CREATE INDEX weakness_series_point_game_idx ON weakness_series_point (game_id);

-- Current estimates, upserted. `double precision` like the Rust f64s: build-profile compares a
-- new estimate with the stored one exactly.
CREATE TABLE weakness_estimates (
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  series_key text NOT NULL,
  k double precision NOT NULL,
  n double precision NOT NULL,
  n_unweighted integer NOT NULL,
  k_unweighted double precision NOT NULL,
  alpha double precision NOT NULL,
  beta double precision NOT NULL,
  mean double precision NOT NULL,
  ci_low double precision NOT NULL,
  ci_high double precision NOT NULL,
  band_median double precision,
  band_p40 double precision,
  confidence double precision NOT NULL,
  excess double precision,
  priority double precision NOT NULL,
  percentile double precision,
  state weakness_state NOT NULL,
  p_imp double precision,
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (account_id, time_class, series_key)
);

-- Append-only. No dedupe key: a series can legitimately transition twice at the same game (a
-- re-analysis flips the state back), and `at_game_id` is nullable. The caller (build-profile)
-- owns dedupe: it appends only when the new state differs from the latest row's `to_state`.
CREATE TABLE weakness_state_history (
  id bigserial PRIMARY KEY,
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  series_key text NOT NULL,
  kind state_row_kind NOT NULL,
  from_state weakness_state,
  to_state weakness_state NOT NULL,
  at_game_id uuid,
  at_played_at timestamptz,
  onset_game_id uuid,
  onset_at timestamptz,
  onset_low timestamptz,
  onset_high timestamptz,
  p_change real,
  rate_before real,
  rate_after real,
  evidence jsonb NOT NULL,
  cause_hints jsonb NOT NULL DEFAULT '[]'::jsonb,
  detector_version text NOT NULL,
  protocol text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX weakness_state_history_series_idx
  ON weakness_state_history (account_id, time_class, series_key, id);

-- Append-only: no unique key, identical rows are kept. Values are `double precision` (exact f64).
CREATE TABLE kpi_snapshot (
  id bigserial PRIMARY KEY,
  user_id uuid,
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  metric text NOT NULL,
  window_kind kpi_window NOT NULL,
  window_key text NOT NULL,
  value double precision,
  value_adjusted double precision,
  n integer NOT NULL,
  ci_low double precision,
  ci_high double precision,
  protocol text NOT NULL,
  computed_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX kpi_snapshot_lookup_idx
  ON kpi_snapshot (account_id, time_class, metric, window_key, computed_at DESC);

-- Reference data (Lichess CC0 dump) ---------------------------------------------------------
CREATE TABLE reference_builds (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  kind reference_kind NOT NULL,
  status build_status NOT NULL DEFAULT 'building',
  source_month text NOT NULL,
  source_sha256 text NOT NULL,
  source_url text,
  params jsonb NOT NULL DEFAULT '{}'::jsonb,
  games_scanned bigint NOT NULL DEFAULT 0,
  protocol text,
  enrichment_protocol text,
  difficulty_model text,
  started_at timestamptz NOT NULL DEFAULT now(),
  finished_at timestamptz,
  activated_at timestamptz,
  error text
);
-- At most one active build per kind.
CREATE UNIQUE INDEX reference_builds_one_active_uq ON reference_builds (kind)
  WHERE status = 'active';

CREATE TABLE opening_explorer (
  build_id uuid NOT NULL REFERENCES reference_builds (id) ON DELETE CASCADE,
  fen_key text NOT NULL,
  band band NOT NULL,
  time_class time_class NOT NULL,
  uci text NOT NULL,
  white integer NOT NULL,
  draws integer NOT NULL,
  black integer NOT NULL,
  PRIMARY KEY (build_id, fen_key, band, time_class, uci)
);

CREATE TABLE reference_games (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  build_id uuid NOT NULL REFERENCES reference_builds (id) ON DELETE CASCADE,
  lichess_game_id text NOT NULL,
  pgn text NOT NULL,
  time_class time_class NOT NULL,
  base_seconds integer,
  increment_seconds integer,
  white_rating integer,
  black_rating integer,
  white_hash text NOT NULL,
  black_hash text NOT NULL,
  result result NOT NULL,
  termination text,
  played_at timestamptz NOT NULL,
  status analysis_status NOT NULL DEFAULT 'pending',
  error text,
  CONSTRAINT reference_games_build_game_uq UNIQUE (build_id, lichess_game_id)
);
CREATE INDEX reference_games_build_status_idx ON reference_games (build_id, status);

CREATE TABLE reference_observations (
  build_id uuid NOT NULL REFERENCES reference_builds (id) ON DELETE CASCADE,
  reference_game_id uuid NOT NULL REFERENCES reference_games (id) ON DELETE CASCADE,
  color color NOT NULL,
  player_hash text NOT NULL,
  band band NOT NULL,
  time_class time_class NOT NULL,
  player_rating integer NOT NULL,
  opponent_rating integer NOT NULL,
  metrics jsonb NOT NULL,
  PRIMARY KEY (reference_game_id, color)
);
CREATE INDEX reference_observations_cell_idx
  ON reference_observations (build_id, band, time_class);

CREATE TABLE band_reference (
  build_id uuid NOT NULL REFERENCES reference_builds (id) ON DELETE CASCADE,
  band band NOT NULL,
  time_class time_class NOT NULL,
  metric text NOT NULL,
  median real NOT NULL,
  p25 real NOT NULL,
  p40 real NOT NULL,
  p75 real NOT NULL,
  pooled_rate real,
  epl_per_event real,
  opps_per_game real,
  opp_slope_per_100 real NOT NULL,
  n_players integer NOT NULL,
  n_games integer NOT NULL,
  low_sample boolean NOT NULL,
  PRIMARY KEY (build_id, band, time_class, metric)
);

-- Openings and repertoire -------------------------------------------------------------------
CREATE TABLE game_openings (
  game_id uuid PRIMARY KEY REFERENCES games (id) ON DELETE CASCADE,
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  color color NOT NULL,
  fen_keys text[] NOT NULL,
  ucis text[] NOT NULL,
  sans text[] NOT NULL,
  line_key text,
  line_name text,
  eco text,
  line_kind line_kind,
  line_entry_ply integer,
  book_exit_ply integer,
  player_deviation_ply integer
);
CREATE INDEX game_openings_account_idx ON game_openings (account_id, time_class, color);
CREATE INDEX game_openings_line_idx
  ON game_openings (account_id, time_class, line_key, line_kind);

CREATE TABLE repertoire_nodes (
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  color color NOT NULL,
  fen_key text NOT NULL,
  games integer NOT NULL,
  stats jsonb NOT NULL,
  PRIMARY KEY (account_id, time_class, color, fen_key)
);

CREATE TABLE opening_lines (
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  color color NOT NULL,
  line_key text NOT NULL,
  -- One line can be both chosen and faced (T16/T18 ruling): the side is part of the key.
  kind line_kind NOT NULL,
  games integer NOT NULL,
  verdict opening_verdict,
  summary jsonb NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (account_id, time_class, color, line_key, kind)
);

-- R2: quick preview evals (R-13). Never in position_evals, moves, game_metrics or series.
CREATE TABLE game_previews (
  game_id uuid PRIMARY KEY REFERENCES games (id) ON DELETE CASCADE,
  protocol text NOT NULL,
  white_accuracy real,
  black_accuracy real,
  moves jsonb NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

-- R2: per-stage timings for every new job kind (`db::stage_metrics`), and the spans ADR 0036
-- logged only in R1 (`engine_stage_ms`, `evaluate_ms` of analyse-game).
ALTER TABLE job_metrics ADD COLUMN stages jsonb;
-- Every job kind now writes rows: per-kind windows (the ETA rate, T30) read by kind first.
CREATE INDEX job_metrics_kind_created_idx ON job_metrics (kind, created_at);
