-- Task 20: the reconciler pages over pending and analysing games by id (keyset). This partial
-- index makes each page an ordered range scan that stops after LIMIT rows, whatever the number of
-- analysed games around them. The status is the leading column, so the two sweeps read disjoint
-- ranges.
CREATE INDEX games_pending_id_idx ON games (analysis_status, id)
  WHERE analysis_status IN ('pending', 'analysing');
