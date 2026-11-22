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
  colours** count: each side of a game is offered to its own cell; a game selected for both
  sides is inserted once and observed for both (Part 2 analyses both colours anyway).
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
  does). The bottom-k players are below the threshold whenever a cell has enough of them, so the
  rate bounds the work without changing the selection; a thin cell is under-filled, not filled
  with other players. The expected size comes from `--expected-games`, else the last full build,
  else 100M (as ADR-R2-10). Admission stops at `max_scan_games` (the build records
  `params.capped` and warns; a correct estimate never reaches it).
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
  `--salt <params.salt>` reproduces its selection from the same dump. No new migration was
  needed. Names are never stored: only the hex hashes (`white_hash`, `black_hash`,
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

### Consequences

- Two default builds of the same month select different players (a fresh salt each). Passing a
  stored build's `params.salt` reproduces its selection exactly; for a given salt and dump the
  selection is deterministic at any thread count (tested).
- With `--explorer --band-reference`, the month is downloaded twice (one pass each). A combined
  sink would save the second download; **parked for Task 35**.
- `protocol` and `enrichment_protocol` stay NULL when the build is created: `lichess-db` does
  not depend on the engine or enrich crates. Task 35's worker, which has both, fills them at
  finalisation; Part 2 records the difficulty model.
- A rare cell (for example B5 classical) can be under-filled; Part 2's low-sample flag and the
  band fallback (Task 17) handle it.

### Alternatives

- **Game-level proportional sampling** (the explorer's rate): rejected, it leaves most players
  with one or two games.
- **First N games of the month**: rejected, time-biased (the T19 ruling).
- **Reservoir sampling of games**: rejected, it samples games, not players.
- **A salt never stored** (Task 21's first version): rejected by the coordinator, because a
  build must be auditable and reproducible. It would have kept the hashes unlinkable to names.
- **Provisional filter on `|RatingDiff|`**: rejected for the reference, it depends on results.
