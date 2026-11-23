-- Weekly focus and goal (ADR 0090): one weakness series a user works on per time class, with a
-- measurable target, a rate per chance over the next `games_n` analysed games after
-- `started_at`. Progress is never stored while the focus runs; the web reads it live from
-- `weakness_series_point`. The row is closed (ended_at, outcome and the result counts) once the
-- focus reaches `games_n` games, or when the user clears or replaces it.
CREATE TYPE focus_outcome AS ENUM ('met', 'not_met', 'inconclusive', 'cleared');

CREATE TABLE focuses (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  series_key text NOT NULL CHECK (length(series_key) BETWEEN 1 AND 200),
  -- The colour the focus counts (ADR 0082): 'both' counts every game.
  side side_scope NOT NULL DEFAULT 'both',
  -- The goal: at most this share of chances over the focus's games.
  target_rate double precision NOT NULL CHECK (target_rate >= 0 AND target_rate < 1),
  games_n integer NOT NULL DEFAULT 20 CHECK (games_n BETWEEN 5 AND 100),
  started_at timestamptz NOT NULL DEFAULT now(),
  -- What the focus started from: the estimate's rate and its unweighted counts (6-month window).
  baseline_rate double precision NOT NULL CHECK (baseline_rate >= 0 AND baseline_rate <= 1),
  baseline_k integer NOT NULL CHECK (baseline_k >= 0),
  baseline_n integer NOT NULL CHECK (baseline_n >= baseline_k),
  -- The peer median the target was weighed against, when the weakness was compared.
  peer_rate double precision,
  ended_at timestamptz,
  outcome focus_outcome,
  -- The counts the focus ended with: chances, events and games inside it.
  result_k integer,
  result_n integer,
  result_games integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT focuses_end_has_outcome CHECK ((ended_at IS NULL) = (outcome IS NULL))
);

-- One running focus per user and time class.
CREATE UNIQUE INDEX focuses_active_idx ON focuses (user_id, time_class) WHERE ended_at IS NULL;
-- The latest focus of a user and time class (the card after a focus closes).
CREATE INDEX focuses_user_tc_idx ON focuses (user_id, time_class, started_at DESC);
CREATE INDEX focuses_account_idx ON focuses (account_id);
