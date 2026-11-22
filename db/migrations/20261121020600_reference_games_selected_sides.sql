-- R2 Task 21 review I1 (coordinator ruling, ADR-R2-11 Part 1).
--
-- A reference game is selected for one player (or for both, when two selected players met). Only
-- the selected side is a reference player: the one-game opponent of a sampled player is a
-- game-weighted, possibly provisional account and must never enter a cell. The selection writes
-- which sides were sampled; Task 32 writes observations only for them.
--
-- Rows written before this migration were observed on both sides, so they keep that meaning
-- (DEFAULT true for the backfill); new rows must state both flags.
ALTER TABLE reference_games
  ADD COLUMN white_selected boolean NOT NULL DEFAULT true,
  ADD COLUMN black_selected boolean NOT NULL DEFAULT true;
ALTER TABLE reference_games
  ALTER COLUMN white_selected DROP DEFAULT,
  ALTER COLUMN black_selected DROP DEFAULT;
ALTER TABLE reference_games ADD CONSTRAINT reference_games_a_side_is_selected
  CHECK (white_selected OR black_selected);
