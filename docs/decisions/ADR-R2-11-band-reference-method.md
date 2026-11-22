# ADR-R2-11: Band reference method

Status: accepted (R2). Placeholder number; the controller renumbers it.

<!-- Part 1 (player selection: bottom-k cells, hashed names, per-build salt) is written by
     R2 Task 21. Task 32 appends the aggregation below. -->

## Part 2: per-game analysis, aggregation and finalisation (Task 32)

### Context

Weakness estimates (Task 12) need a band prior (`pooled_rate`), quartiles (`p25`, `p40`,
`median`, `p75`) and `epl_per_event`. KPIs (Task 15) need quartiles and an opponent slope. Both
come from the reference games that Task 21 samples per band × time class.

### Decision

- **Both colours of every reference game are analysed**, with the user pipeline: our own
  `sf19-n300k-v1` evaluation (never the dump's `[%eval]`, which is another protocol), enrichment
  of both colours (`enr-v1`), then features and metrics per side. One `reference_observations`
  row per side that has both ratings; a side without ratings has no band and no gap, so it gives
  no observation (counted in `job_metrics`).
- **Binomial rows use a Beta-binomial fit, not raw per-player rates.** Per player, events and
  opportunities are summed over the player's games in the cell. `pooled_rate = Σk / Σn`. The
  quartiles are those of the fitted Beta of per-player rates: at 10 games per player, raw rates
  are mostly binomial noise and would spread the quartiles far too wide.
  `epl_per_event = Σepl / Σk` (weakness rows; `None` without events, never 0) and
  `opps_per_game = Σn / games with evidence`.
- **Continuous rows** use the per-player mean, then type-7 quantiles of those means. The rating
  (`Last`) gets no reference.
- **Opponent slope:** `opp_slope_per_100` is the OLS slope of the per-game value (a rate's
  per-game `k/n`) on `(opponent − own) / 100`, pooled over the cell's games. Task 15 subtracts
  `slope × mean gap` for its adjusted value. When the slope cannot be estimated (fewer than 2
  games, or no spread in the gap) it is unknown: `None`, stored NULL. Every reader treats that as
  "no adjustment". Migration `20261121020500` made the column nullable, and it refuses NaN and
  infinities with a CHECK. The value is never NaN and never 0 (coordinator ruling).
- `n_players` counts players with evidence for the row; `low_sample = n_players < 10`, and
  low-sample rows are never served (Task 17's per-metric fallback).
- **Version gate:** an observation whose metrics version is not `PLAYER_GAME_METRICS_VERSION` is
  stale and left out (counted).
- **Comparability only within the same protocol, enrichment and difficulty model.** The build
  records `protocol`, `enrichment_protocol` and `difficulty_model`. Difficulty is Maia only
  (owner override): the Maia model that produced a game's values is recorded on the build in that
  game's transaction. A game whose model differs from the build's fails alone. Without Maia the
  column stays NULL, and difficulty-based detectors treat those plies as unknown (`pending_maia`;
  plies ≤ 10 are `not_applicable`).
- **Pending Maia is re-enriched, never frozen in silently** (coordinator ruling):
  - A game left with plies pending because Maia was *unavailable* is flagged in
    `reference_games.difficulty_pending_at`, which keeps the first such time. With Maia *not
    configured* nothing could answer later, so the game is not flagged.
  - Finalisation waits while a flagged game is younger than `MAIA_GRACE` (24 h).
  - The recurring `reference-difficulty-sweep` job (`reenrich_reference_games`) runs once Maia
    is healthy. It re-runs flagged games of building builds (evals and probes come from the
    caches) and rewrites their observations and flags. It then finalises every finished build,
    including one whose grace ran out while Maia stayed down.
  - A build finalised with flagged games keeps those plies out of the difficulty metrics'
    denominators: a pending ply is never a W11 opportunity and never an estimate. The count is
    reported as `maia_pending_games`.
- **Provisional chess.com offsets.** The reference is on the Lichess scale. chess.com players
  are banded through `CHESSCOM_TO_LICHESS_OFFSET`, which is provisional and is to be replaced by
  pooled user data later.
- **Finalisation** runs when no game of the build is `pending` or `analysing`. Every worker that
  finishes a game calls it, so it is idempotent: under `pg_advisory_xact_lock(hashtext(build_id))`
  an active build returns `AlreadyActive`, any other non-building status is a no-op, and
  outstanding games return `Pending`. The rows are written and the build activated
  (`activate_build_in`) in one transaction.
- **Minimum evidence:** a build with no usable (not low-sample) weakness row is **failed, never
  activated**. Activating it would supersede the previous build and delete its rows, leaving users
  with fewer benchmarks.
- **Supersede rule** (review I1, coordinator ruling):
  - A new build replaces the active one only if it has at least as many usable cells. A usable
    cell is a band × time class × metric row that passes minimum evidence (not low-sample).
  - A thinner build is finalised as `failed` with the reason "thinner than active build `<id>`:
    X vs Y usable cells", and the active build stays untouched.
  - The first build ever needs only the evidence gate.
  - The comparison and the activation run in one transaction, under one global advisory lock
    (`hashtext('band-reference-activate')`), and the active build is re-read under it. Two
    builds finalising at once are compared in turn, so a thinner build can never supersede one
    that activated a moment earlier.
  - Locks are always taken in this order: per build, then global, then the activation's kind
    lock.
  - A dump that is not newer than the active one can never activate. It is failed
    (`OlderThanActive`) rather than retried for ever.
  - This stops a degraded build from replacing a complete one, for example one where most
    games failed in a Stockfish outage, or a selection bug that covered a single band.
- **One Maia model per build.** The per-game model is the model of the first answered ply.
  This is fine while one batch call is answered by one model. If Maia batches can ever span a
  model reload, the check must look at every ply (review m7).
- **Bad games never block the build.** Poison games (an illegal PGN or records) fail, unsupported
  or too-short ones are skipped, and engine errors are retried and then failed on the final
  attempt or by `on_dead`. A job whose game row is gone (its build was superseded) is a no-op.

### Consequences

- A band where nobody had an event has `median = 0`, `pooled_rate = 0` and
  `epl_per_event = None`. Task 12 then has no benchmark for it (it needs an EPL), so a zero-event
  player there never surfaces a weakness (T12 ruling, tested).
- A Maia outage delays a build by up to 24 h. Only a longer outage leaves the build's
  difficulty-based rows with fewer opportunities, and the denominators are still honest.
- The finaliser's advisory lock only avoids duplicate work. Correctness comes from the
  idempotent upserts, the activation's own kind lock and the `NotBuilding` handling.
- A build that would be activated without a SHA-256 fails in `activate_build_in`
  (`MissingSourceSha`). Its last job is then retried and dead-lettered. Task 21 must set the SHA
  before enqueueing games.
