# EXP-0006 development

The challenger is chosen here, on development seeds, before the experiment is registered. Nothing in this folder is confirmatory; the registered analysis draws fresh seeds (`EXP-0006`), disjoint from these (`EXP-0006-dev`).

## The grid

Each rule is a `profile_core::stats::variant::PriorRule`, selected with `ALGO_VARIANT`:

- the prior's cap in games, `prior_games` ∈ {none, 8, 4, 2, 1}: the band prior is worth at most that many of the player's own games, counted as the data are (`n0 = min(n0[key], G × n / games_in_window)`);
- the counts: the recency-weighted sums (production), their effective sample size (`ess`, Kish: the weighted rate over `(Σ w n)² / Σ w² n` chances), or that divided by the Pearson dispersion of the per-game counts (`ess,disp`, clamped to [1, 10]).

Fifteen rules with the champion. Empirical-Bayes prior strengths from the peers' quartiles are not in the grid: the quartiles of a real peer row carry the peers' own sampling noise, and in EXP-0002's design they are assumptions (0.6, 0.85 and 1.5 times the rate), so a rule fitted to them would be fitted to our assumption. Partial pooling across players is EXP-0003's question (#11).

## The design

EXP-0002's generator, imported from its frozen `analysis.py`, on the seed namespace `EXP-0006-dev`: 3 bands × 5 ρ × 2 dispersions, 100 players a cell, two games a day, checkpoints 30, 60, 120 and 250 games. One profile call per player and checkpoint, through `cli lab profile` with `ALGO_VARIANT`.

## The selection rule (written before the first run)

1. Score every rule by D, the largest |coverage − 0.80| over the 20 primary cells of EXP-0002 (5 keys × 4 checkpoints, ρ ∈ {2, 4, 8}, cells weighted equally).
2. Keep the rules whose ρ = 1 coverage deviation |coverage − 0.80| is at most the champion's plus 0.05, and whose share of credible estimates among players at or below the peers' rate (ρ ∈ {0.5, 1}) is at most the champion's plus 0.01.
3. Among them take the smallest D; rules within 0.01 of it are tied, and the simplest of the tied wins: fewer options first, then the larger cap in games (the one closest to the champion).

The chosen rule becomes `PriorRule::CHALLENGER` in `backend/crates/profile-core/src/stats/variant.rs`, and does not change afterwards.
