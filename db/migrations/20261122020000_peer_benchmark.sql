-- The peer benchmark (ADR 0080): the opponent sides of an account's analysed games as its
-- reference group, on the account's own rating scale.

-- The opponent side's metrics of one analysed game, computed from the stored PGN and the cached
-- main evals without enrichment. `source_created_at` is the player's `game_metrics.created_at`
-- it was computed with: a re-analysed game (a newer row) recomputes it.
CREATE TABLE opponent_game_metrics (
  game_id uuid PRIMARY KEY REFERENCES games (id) ON DELETE CASCADE,
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  -- Lower-case username: one player however often they were faced.
  opponent text NOT NULL,
  -- Pre-game ratings of the opponent and of the account's player.
  opponent_rating integer NOT NULL,
  player_rating integer NOT NULL,
  metrics jsonb NOT NULL,
  metrics_version integer GENERATED ALWAYS AS (COALESCE((metrics->>'version')::int, 0)) STORED,
  source_created_at timestamptz NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX opponent_game_metrics_account_tc_idx
  ON opponent_game_metrics (account_id, time_class);

-- The benchmark the account × time class's last build-profile compared with. `source` is
-- 'peers' (with its band and counts) or the legacy 'lichess_bands' (no band of its own).
CREATE TABLE account_benchmark (
  account_id uuid NOT NULL REFERENCES linked_accounts (id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  source text NOT NULL CHECK (source IN ('peers', 'lichess_bands')),
  center_rating integer,
  rating_low integer,
  rating_high integer,
  n_players integer CHECK (n_players >= 0),
  n_games integer CHECK (n_games >= 0),
  computed_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (account_id, time_class),
  CHECK (source <> 'peers' OR (center_rating IS NOT NULL AND rating_low IS NOT NULL
    AND rating_high IS NOT NULL AND n_players IS NOT NULL AND n_games IS NOT NULL))
);

-- Its rows, as `band_reference` (low-sample rows stored, never served).
CREATE TABLE account_benchmark_stat (
  account_id uuid NOT NULL,
  time_class time_class NOT NULL,
  metric text NOT NULL,
  median double precision NOT NULL,
  p25 double precision NOT NULL,
  p40 double precision NOT NULL,
  p75 double precision NOT NULL,
  pooled_rate double precision,
  epl_per_event double precision,
  opps_per_game double precision,
  opp_slope_per_100 double precision,
  n_players integer NOT NULL CHECK (n_players >= 0),
  n_games integer NOT NULL CHECK (n_games >= 0),
  low_sample boolean NOT NULL,
  PRIMARY KEY (account_id, time_class, metric),
  FOREIGN KEY (account_id, time_class) REFERENCES account_benchmark (account_id, time_class)
    ON DELETE CASCADE
);
