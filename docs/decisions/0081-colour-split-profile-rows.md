# 0081. The colour split: profile rows per side the player had

Date: 2026-11-22. Status: accepted (next round, task T1). Builds on ADR-R2-5 (priority and
`MIN_GAMES`), ADR-R2-6 (timeline states), ADR-R2-11 (band reference aggregation), ADR-R2-23
(per-metric band fallback) and ADR 0080 (the chess.com peer benchmark, in parallel).

## Context

The owner wants every stat broken down for White and Black. The profile tables held one row per
account × time class (× metric or series), computed over every game. `GameContext.player_color`
was already loaded for every analysed game, and every weakness series point carries its game id,
so a per-colour profile needs no engine work: only the stored metrics and series points,
filtered by colour.

## Decision

1. **One dimension, `side_scope ('both', 'white', 'black')`** (migration
   `20261123000000_colour_split.sql`). `kpi_snapshot`, `weakness_estimates` and `band_reference`
   get `side NOT NULL DEFAULT 'both'`, so every existing row is `both`. The side joins
   `kpi_snapshot_lookup_idx` and the primary keys of `weakness_estimates`
   `(account_id, time_class, side, series_key)` and `band_reference`
   `(build_id, band, time_class, side, metric)`. Rust's `profile_core::types::Side` mirrors it.
2. **build-profile computes three times.** Estimates and KPI snapshots (with their `area:*`
   rows) are computed for `both` (unchanged), and for the games the player had White, then
   Black, in: that side's series points, games-in-window count and weekly windows. Sessions are
   computed once over every game (a session mixes colours). Every binomial key gets a row per
   side, a side without games included, so each side's row set is complete.
3. **The same minimum evidence, counted per side.** `MIN_GAMES` (30) gates ranking on the
   side's own games in the window; a snapshot's `n` is the side's own count, so the web's
   `KPI_MIN_GAMES` (10) and per-KPI minimums apply per side unchanged.
4. **The timeline stays `both` only.** Per-colour states would halve the evidence behind every
   onset and transition. `weakness_state_history` has no side. A colour's estimate rows carry
   the account's (both) state and a null `P_imp` (a timeline quantity). Events
   (`profile.updated`, `weakness.changed`) come from the both side only, as before.
5. **A side is benchmarked only by its own rows.** `load_band_reference_for(band, tc, side)`
   reads one side; the per-metric fallback moves across bands, never across sides. A `both`
   reference is never applied to one colour: no colour rows means no colour benchmark
   (`NoBenchmark`, null band fields). The Lichess aggregation now also writes `white` and
   `black` rows from the sides that had that colour, with their own player counts and
   `low_sample`. The build's evidence gate and supersede rule count `both` rows only, so a
   build's activation is decided as before. The peer benchmark (ADR 0080) should write its
   per-colour rows from the opponents' sides with the same rule.
6. **Existing profiles rebuild once, without the engine.** There is no profile version to bump:
   `PLAYER_GAME_METRICS_VERSION` describes the per-game metrics, which did not change, and
   bumping it would also make every reference observation stale. Instead the worker start runs
   `requeue_colour_split`: every account × time class with `both` estimates but no `black` one
   gets a build-profile job (deduped). The build reads the stored rows, so it is idempotent and
   the pair is never due again.
7. **Readers keep reading `both`.** The coaching input, the weekly input, the scheduler's
   precision check, the timeline's stored states and the web read models (`latestSnapshots`,
   the weakness summaries) filter `side = 'both'`, so every page is unchanged. Per-side UI and
   the CoachingInput `by_color` come later (T2b, T3, T4).

## Consequences

- About three times the estimate rows and up to three times the snapshot rows per build; the
  build's CPU part grows accordingly (still one load, one transaction).
- A colour's ranking can differ from the account's: Black may be unranked (fewer than 30
  games) while both is ranked. That is the honest reading of the evidence.
- Until a band reference build or the peer benchmark writes colour rows, colour estimates and
  area scores are unbenchmarked.
