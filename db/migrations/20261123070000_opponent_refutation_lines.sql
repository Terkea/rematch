-- ADR 0101: the opponent sides of the peer benchmark now carry refutation lines from the cached
-- main-eval PVs, so W1 is measured on both sides by the same rule. The cached sides were
-- computed without them (W1's decisive moves left out of its chances), at the same metrics
-- version, so they are dropped here; the next build-profile recomputes them from the cached
-- evals, with no engine.
DELETE FROM opponent_game_metrics;
