# ADR-R2-11: Band reference method

Status: accepted (R2). Placeholder number; the controller renumbers it.

## Part 1: reference-player selection (Task 21)

### Context

The band reference is the benchmark that a user's weakness rates and KPIs are compared with,
per band × time class. It must describe typical *players* of a band, not typical *games*:
a game sample is dominated by the most active accounts, and per-player rates (Part 2 fits a
Beta-binomial over players) need several games of the same player. The source is one Lichess
monthly dump, which is in time order, about 100M games, 30+ GB compressed, and read once,
streamed. It has no provisional flag and must not leave usernames in our database.

### Decision

- **Player-sampled cells.** A cell is `(band_of(the player's rating at the game), time class)`,
  with the six bands B0–B5 and bullet, blitz, rapid and classical (no correspondence). Each
  cell selects **20 players × 10 games** (`players_per_cell`, `games_per_player`). **Both
  colours** count: each side of a game is offered to its own cell. **Only the sampled side is
  a reference player** (T21 review I1): `reference_games.white_selected` / `black_selected`
  record which side(s) were selected, and Part 2 writes observations only for them. The
  one-game opponent of a sampled player is game-weighted (biased toward high-volume accounts),
  never passed the activity rule and may be provisional, so it never enters a cell. A game
  selected for both sides is inserted once with both flags.
- **Bottom-k by a salted hash, one pass.** `h = sha256(salt ‖ lower(name))`. Per cell a max-heap
  holds at most `players_per_cell × oversample` (20 × 20 = 400) candidates with the smallest `h`.
  A player evicted from the heap never re-enters: its hash is fixed and the heap's maximum only
  falls. Each held player buffers at most 10 games: those with the smallest
  `sha256(salt ‖ "game:" ‖ id)`, so a player's games spread over the month instead of its first
  days. At the end each cell takes, by ascending `h`, the first 20 candidates with 10 buffered
  games that also pass the activity rule below. The oversample of 20 covers candidates who
  never reach 10 games in the cell.
- **Unbiased across the month, deterministic.** The hash does not depend on when a game was
  played, and the whole month is scanned (it is drained for the SHA-256 anyway, R-9), so
  neither the first hours nor any weekday is favoured. Every decision runs in the pipeline's
  in-order merge, so the selection is identical at any thread count, for a given salt and dump.
- **Bounded work: a player-level rate, not a game-level one.** Sampling games would thin each
  player's games below 10. Instead, a side is admitted only when `fraction(h) < player_rate`,
  with `player_rate = min(1, max_scan_games / (2 × expected games))` and `max_scan_games =
  3 000 000` (each admitted player brings all their games; a game passes when either side
  does). The expected size comes from `--expected-games`, else the last full build, else 100M
  (as ADR-R2-10).
- **The rate changes sparse cells** (T21 review I2, corrected). A cell keeps its exact bottom-k
  only when at least `players_per_cell` of its *eligible* players fall below the threshold:
  about `20 / player_rate` eligible players, roughly 1 300 at a rate of 0.015 (100M games).
  Sparse cells (classical, B5 rapid) plausibly have fewer, and the activity rule thins dense
  ones too, so such a cell is **under-filled** (fewer than 20 players; never filled with
  others). Every cell's fill (`target`, `candidates`, `full`, `eligible`, `chosen`) is recorded
  in `params.cells`, `params.under_filled_cells` counts the short ones, and each is logged. An
  adaptive per-cell rate is deferred; Part 2's evidence gate and low-sample flag already refuse
  thin cells.
- **Admission cap with a 1.5× margin; a capped build never activates** (T21 review I3).
  Admission stops at `1.5 × max_scan_games` (`params.admission_cap`), so a month up to half
  again as large as estimated still reads to the end. Stopping admission is a first-N cut of
  the rest of the month (late players missing, activity counts leaning early), so a build that
  reaches the cap records `params.capped = true`, enqueues nothing and fails, as capped explorer
  builds do not activate until `--allow-capped` exists.
- **Memory bound** = cells × candidates × games × PGN size = 24 × 400 × 10 × about 2 KB, about
  200 MB, logged when the pass starts and stored in `params.memory_bound_bytes`. A game held for
  both colours is shared. The per-(player, time class) game counters are bounded by the admitted
  sides.
- **Provisional or barely active players: a result-independent rule.** A selected player needs
  at least **20 games in that time class within the dump month** (`min_games_in_time_class`,
  counted across bands). Lichess ratings stop being provisional as the rating deviation falls
  with games played, so an account this active in the time class is unlikely to be provisional,
  and the rule also drops accounts that barely play the class. It counts games only, never
  results or rating changes, so it cannot select players by how their games went. **The value 20
  is an unverified estimate**: it was not checked against Lichess rating deviations. The
  `|RatingDiff| ≥ 25` heuristic (ADR-R2-10) stays off here: a rating change grows with the
  surprise of the result, so it filters partly by result.
- **Other filters:** rated games with a real clock and both ratings, `[%clk]` comments required
  (the reference metrics use clocks, as the users' games have them), no BOT on either side, no
  `Abandoned` or `Rules infraction` terminations (ADR-R2-9).
- **Hashed names, per-build salt stored for audit (coordinator ruling).** The salt is 32 bytes
  of fresh OS randomness per build, or the `--salt <64 hex>` override, and is recorded in the
  build's `params.salt` (where `capped` is recorded), so a build is auditable and
  `--salt <params.salt> --expected-games <params.expected_games>` reproduces its selection from
  the same dump (the player rate depends on the expected size too). Names are never stored: only the hex hashes (`white_hash`, `black_hash`,
  `player_hash`), and the stored PGN has its `White`/`Black` tags replaced by `?`. Because the
  salt is stored next to the hashes, a hash can be matched to a name by hashing candidate
  Lichess usernames; the hashes are pseudonyms that group a player's games, not a secret. (The
  Lichess game id stays too, so the public game itself can be looked up on Lichess.)
- **Verification before selection.** The pass streams the whole month, drains it, verifies the
  SHA-256, sets `source_sha256`, and only then inserts the games (`pending`) and enqueues one
  `analyse-reference-game` job per game (priority 200, dedupe `refgame:<id>`).
  `enqueue_reference_games` refuses a build without a source SHA. A mismatch fails the build
  with no reference games. A selection with no game fails the build. The build stays
  `building` until Part 2's finaliser activates it.
- **Resumable enqueue** (T21 review I4). The rows commit in one transaction and the jobs follow
  in batches, so a crash in between leaves pending rows without jobs. The jobs are built from
  the database (`resume_reference_enqueue`: every `pending` game of a verified `building`
  build), and the `refgame:<id>` dedupe makes a re-run safe. Task 35 exposes it as `lichess-db resume`
  (ADR-R2-25).

### Consequences

- Two default builds of the same month select different players (a fresh salt each). Passing a
  stored build's `params.salt` reproduces its selection exactly; for a given salt and dump the
  selection is deterministic at any thread count (tested).
- With `--explorer --band-reference`, the month is downloaded twice (one pass each). A combined
  sink would save the second download. Task 35 parked it again (ADR-R2-25): it is a lane D
  change to the pipeline, and no dump build has run yet.
- `protocol` and `enrichment_protocol` stay NULL when the build is created: `lichess-db` does
  not depend on the engine or enrich crates. The finaliser fills them at activation
  (`sf19-n300k-v1`, `enr-v1`; Task 35, ADR-R2-25); Part 2 records the difficulty model.
- A rare cell (for example B5 classical) can be under-filled, visibly in `params.cells`; Part
  2's low-sample flag, its evidence gate and the band fallback (Task 17) handle it.
- One migration (`20261121020600_reference_games_selected_sides`) adds the two selection flags,
  with a check that at least one side is selected.

### Alternatives

- **Game-level proportional sampling** (the explorer's rate): rejected, it leaves most players
  with one or two games.
- **First N games of the month**: rejected, time-biased (the T19 ruling).
- **Reservoir sampling of games**: rejected, it samples games, not players.
- **A salt never stored** (Task 21's first version): rejected by the coordinator, because a
  build must be auditable and reproducible. It would have kept the hashes unlinkable to names.
- **Provisional filter on `|RatingDiff|`**: rejected for the reference, it depends on results.

## Part 2: per-game analysis, aggregation and finalisation (Task 32)

### Context

Weakness estimates (Task 12) need a band prior (`pooled_rate`), quartiles (`p25`, `p40`,
`median`, `p75`) and `epl_per_event`. KPIs (Task 15) need quartiles and an opponent slope. Both
come from the reference games that Task 21 samples per band × time class.

### Decision

- **Both colours of every reference game are analysed**, with the user pipeline: our own
  `sf19-n300k-v1` evaluation (never the dump's `[%eval]`, which is another protocol), enrichment
  of both colours (`enr-v1`), then features and metrics per side. One `reference_observations`
  row per **sampled** side (`white_selected` / `black_selected`, Part 1) that has both ratings;
  the unsampled opponent gives no observation (`sides_not_sampled` in `job_metrics`), and a side
  without ratings has no band and no gap, so it gives none either.
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
