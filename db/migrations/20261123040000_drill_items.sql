-- The missed-moments trainer (ADR 0090): the player's own positions before a missed mate, a
-- missed tactic, a blunder or an early blunder, drilled on a Leitner schedule. Items belong to a
-- user (never shared across users, even for a shared linked account), and grading reads only
-- what is stored here, so the web needs no engine.
CREATE TABLE drill_items (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE,
  time_class time_class NOT NULL,
  game_id uuid NOT NULL REFERENCES games(id) ON DELETE CASCADE,
  ply integer NOT NULL CHECK (ply > 0),
  -- The most specific source (W4.missed, W3.shallow, W3.deep, W7.traps, then blunder), and every
  -- source the position belongs to (a missed mate is often a blunder too).
  source_key text NOT NULL,
  source_keys text[] NOT NULL,
  -- The position before the player's move, and the move they played in the game.
  fen text NOT NULL,
  played_uci text NOT NULL,
  -- The engine's move, its line (UCI, from move_features.best_pv) and every move that keeps the
  -- outcome (ADR 0090: a mate within best+1, else at most 5 win% points below the best).
  solution_uci text NOT NULL,
  solution_line text[] NOT NULL,
  accepted_uci text[] NOT NULL,
  -- The engine's forced mate for the mover (moves), or null when the best move does not mate.
  mate_in integer,
  win_before double precision NOT NULL,
  win_after double precision NOT NULL,
  -- Motif tags of the engine's line (move_features.best_line.tags): the pattern the efficacy
  -- panel compares in later games.
  tags text[] NOT NULL,
  -- The efficacy holdout: about 20% of items, chosen once by a hash of the position, never drilled.
  held_out boolean NOT NULL,
  -- Scheduling (UTC days): null `introduced_on` is a new item not yet shown.
  streak smallint NOT NULL DEFAULT 0,
  clean_passes smallint NOT NULL DEFAULT 0,
  due_on date,
  introduced_on date,
  last_attempt_on date,
  created_at timestamptz NOT NULL DEFAULT now(),
  retired_at timestamptz,
  CONSTRAINT drill_items_source_check CHECK (
    source_key IN ('W4.missed', 'W3.shallow', 'W3.deep', 'W7.traps', 'blunder')
    AND source_keys <@ ARRAY['W4.missed', 'W3.shallow', 'W3.deep', 'W7.traps', 'blunder']::text[]
    AND source_key = ANY (source_keys)),
  CONSTRAINT drill_items_position_unique UNIQUE (user_id, game_id, ply)
);
-- Today's session and the "due tomorrow" count.
CREATE INDEX drill_items_due_idx
  ON drill_items (user_id, account_id, time_class, due_on)
  WHERE retired_at IS NULL AND NOT held_out;

CREATE TABLE drill_attempts (
  id bigserial PRIMARY KEY,
  item_id uuid NOT NULL REFERENCES drill_items(id) ON DELETE CASCADE,
  user_id uuid NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  at timestamptz NOT NULL DEFAULT now(),
  correct boolean NOT NULL,
  -- Chosen before the reveal (hypercorrection: a confident error is corrected best).
  confidence text NOT NULL CHECK (confidence IN ('sure', 'guess')),
  move_uci text NOT NULL,
  ms integer CHECK (ms IS NULL OR ms >= 0)
);
CREATE INDEX drill_attempts_item_idx ON drill_attempts (item_id, at);
CREATE INDEX drill_attempts_user_idx ON drill_attempts (user_id, at);
