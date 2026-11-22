-- R2 Task 31 fix round 1 (ADR-R2-24): a durable "recompute needed" mark for games whose stored
-- enrichment changed after their metrics were computed.
--
-- The pending-Maia sweep (Task 25) writes Maia's difficulty into move_features row by row. The
-- game's game_metrics, game_openings and series rows must then be recomputed. The mark is set by
-- a trigger in the same statement (so in the same transaction) as the difficulty write, so a
-- sweep cut off by a cancel, a DB error or a crash before its recompute loses nothing: the next
-- sweep recomputes every marked game and clears the mark in the recompute's transaction.
--
-- profile_stale_at: when the game's derived rows went stale; NULL when they are current. The
--   analyse hook clears it (it recomputes everything); the recompute clears it only if it was not
--   set again meanwhile (`profile_stale_at <= read value`).
ALTER TABLE games ADD COLUMN profile_stale_at timestamptz;

-- The recompute reads the marked games oldest first; partial, so its size is the backlog.
CREATE INDEX games_profile_stale_idx ON games (profile_stale_at, id)
  WHERE profile_stale_at IS NOT NULL;

-- Statement-level with transition tables: one games UPDATE per move_features statement.
CREATE FUNCTION mark_profile_stale() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  UPDATE games g SET profile_stale_at = now()
  FROM (
    SELECT DISTINCT n.game_id
    FROM new_rows n
    JOIN old_rows o ON o.game_id = n.game_id AND o.ply = n.ply
    WHERE n.difficulty_status = 'ok' AND o.difficulty_status IS DISTINCT FROM 'ok'
  ) changed
  WHERE g.id = changed.game_id;
  RETURN NULL;
END;
$$;

CREATE TRIGGER move_features_mark_profile_stale
  AFTER UPDATE ON move_features
  REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
  FOR EACH STATEMENT EXECUTE FUNCTION mark_profile_stale();
