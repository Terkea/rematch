-- ADR 0081: a peer rate row is the pooled rate of the opponents' chances; the web names it with
-- the raw count of those chances in the window. NULL for mean rows and for the Lichess bands.
ALTER TABLE account_benchmark_stat
  ADD COLUMN n_chances integer CHECK (n_chances >= 0);
