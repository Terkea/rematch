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

## The outcome (`dev-results.json`, `dev.log`)

The champion's D on these seeds is 0.69 (W4.missed at 30 games, 0.11), its ρ = 1 deviation 0.080 and its false-credible share 0.0239, so a rule is eligible with a ρ = 1 deviation of at most 0.130 and a false-credible share of at most 0.0339. Eleven rules are eligible; the `ess` rules without `disp` at 8, 4, 2 and 1 games are not (false-credible shares 0.0345 to 0.0378).

| Rule | D | Worst cell | Cells in [0.75, 0.85] | ρ = 1 coverage | False-credible share |
|---|---|---|---|---|---|
| champion | 0.693 | W4.missed@30 | 5 | 0.880 | 0.0239 |
| games=4 | 0.099 | W1@30 | 9 | 0.842 | 0.0315 |
| games=2 | 0.096 | W4.missed@250 | 10 | 0.837 | 0.0328 |
| games=1 | 0.095 | W4.missed@250 | 10 | 0.834 | 0.0333 |
| games=4,ess,disp | 0.068 | W1@30 | 19 | 0.815 | 0.0317 |
| games=2,ess,disp | 0.050 | W5@30 | 20 | 0.809 | 0.0328 |
| games=1,ess,disp | 0.050 | W5@30 | 20 | 0.806 | 0.0334 |
| ess,disp | 0.694 | W4.missed@30 | 7 | 0.849 | 0.0240 |

The smallest D, 0.050, is shared by `games=2,ess,disp` and `games=1,ess,disp`; both have three options, and the larger cap wins. The challenger is `dev:games=2,ess,disp`: the band prior is worth at most two of the player's games, the weighted data count at their effective sample size, divided by their dispersion. Its worst cell is W5 at 30 games (0.85), a key whose flat prior none of the options touches. Its false-credible share is 0.0089 above the champion's, close to the 0.01 margin the registration guards; that is the price of the calibration, since a calibrated 80% range excludes the true rate on the low side one time in ten.
