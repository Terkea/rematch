# ADR-R2-18. Build-profile and the CoachingInput builder

Status: accepted (build-profile, Task 33; the builder, Task 34).

## Context

R2 turns analysed games into a profile: estimates, a state timeline, KPI snapshots and opening
lines (build-profile, Task 33). It then turns that profile into the M3 `CoachingInput`
(the builder, Task 34). The AI coach (R3) and the web read only that contract.

## Build-profile flow (Task 33, TS M2 Task 25)

1. Record `started_at` from the job's clock.
2. Read the band reference before the lock, since it is reference data. Take
   `pg_advisory_xact_lock(hashtext('build-profile:<account>:<tc>'))`. Then do every account read
   on that transaction's connection: the `game_metrics` fingerprint, the current-version games
   (version gate), their series points, the band (re-read; when it moved, the reference is read
   again), line keys, stored history, stored estimate states and the latest snapshots.
   - **The lock comes first, then the load (T33 review M1).** The earlier optimistic
     load-and-retry was dropped. One build holds one connection.
3. Run the CPU-heavy part in `spawn_blocking`:
   - one `estimate_weakness` per binomial key;
   - `evaluate_series` per series;
   - KPIs and area scores.

   A panic there is `Unrecoverable` (code `panic`), because it is deterministic.
4. Persist and commit.
5. Publish the events after the commit (best effort; state lives in Postgres).
6. Run `rebuild_repertoire`. It has its own lock and transaction, and holds two connections
   (Task 18's code).
7. Follow-up: the job first gives up its own dedupe key (fenced by lease token and `running`).
   It then re-enqueues itself when the fingerprint changed or a game has
   `analysed_at > started_at`. Both checks are needed: `analysed_at` is the analysis
   transaction's start, so it can precede `started_at` for a game that committed after the load.
   - The key release goes through `Queue::release_dedupe_key` (Task 35, ADR-R2-25).
8. Enqueue `schedule-analysis` and record the `job_metrics` row (`build-profile`, 6 stages).

Mapping choices recorded from T33 (§Concerns 4):

- **Every ISO week is recomputed**, not only the weeks with a game analysed since the last
  snapshot. The TS rule is unsafe on `analysed_at` timestamps (point 7). Change detection makes
  the stored result equal to the TS rule's, plus the weeks it would miss. The cost is CPU,
  about 69 rows × weeks; the T35 smoke times it.
- **Series references:**
  - weakness keys use `weakness:<k>` with `n0(k)` and `min_evidence(k)`;
  - `motif.missed:<t>` uses `motif_miss_rate:<t>` with the W3.shallow constants;
  - `motif.allowed:*`, `opening:*` and `endgame:*` have no band statistic, so they are only
    ever `not_an_issue`;
  - a statistic without a pooled rate is no reference.
- **Mindset has no benchmark** as a static rule: no `profile.updated` for mindset.
- **Area scores are stored for every window** as `area:<area>` rows (score, `n`, no interval),
  not only for `last50`.
- **Cause hints are computed on onset rows only.** They compare up to 50 series games before
  the onset with the games from the onset to the confirming game.
- **`onsetAt` is the current episode's onset** (T33 N3, fixed in T34). It is the latest onset row
  into the current state dated at or after the transition that entered that state. An episode
  entered without its own onset row reports `null`, never an earlier episode's onset. This is
  pinned by `onset_at_names_only_the_current_episode`.
  - It **fails closed** (T34 review m1): without a latest transition, or when the latest
    transition does not enter the current state, it reports `null`. Pinned by
    `onset_fails_closed_when_the_last_transition_enters_another_state`.

### Event payloads

Both events are account-scoped (`user_events.account_id`). They carry the player's own
statistics, so with R3's user scoping they go only to the linked account's owner.

- `profile.updated`: `{timeClass, area, kpis: [{metric, value, n, ciLow, ciHigh}]}`, with the
  area's `last50` rows including `area:<area>`. One event is sent per benchmarked area with an
  appended row in any window.
- `weakness.changed`: `{id: seriesKey, timeClass, state, fromState, onsetAt}`. One event is
  sent per series whose current state differs from its stored current state, read under the
  lock. That stored state is:
  - `weakness_estimates.state` for weakness keys;
  - the latest transition otherwise;
  - `not_an_issue` when nothing is stored (R-7).

  A replay that only moves rows publishes nothing.

### Append-only rules, and history as the current replay

- **`kpi_snapshot` is append-only.** A row is appended when `(value, n, value_adjusted, ci_low,
  ci_high)` differs exactly from the latest row (highest id) of that metric and window. The
  columns are `double precision`, so f64 round-trips.
- **`weakness_state_history` is the current replay, not append-only** (T33 ruling I1 a). Each
  build replays every series from its first game. Within the build's transaction, and for the
  current `DETECTOR_VERSION` only, stored rows that are not in the replay are deleted, and
  replayed rows that are not stored are inserted.
  - Row identity is `(series_key, kind, from_state, to_state, at_game_id, onset_game_id)`. These
    are text and uuid columns only, so the `real` columns are never compared.
  - The delete is fenced by account, time class and detector version.
  - Rows of other detector versions are kept.
  - The history therefore never contradicts itself, and the builder reads its changes from it.
- **Stale migration comment (T33 N5).** The comment on `weakness_state_history` in
  `db/migrations/20261121020000_profile_engine.sql` still says "Append-only … The caller
  (build-profile) … appends only when the new state differs from the latest row's `to_state`".
  That comment is superseded by the rule above. The migration file is deliberately not edited:
  sqlx stores a checksum of every applied migration, so changing even a comment makes every
  database that has applied it fail validation at the next start. The comment can be corrected
  only in a new migration, for example a `COMMENT ON TABLE`, the next time the coordinator adds
  one.

## The CoachingInput builder (Task 34)

`profile::coaching::build_coaching_input(pool, account_id, tc, now)` returns a validated
`coaching_input::CoachingInput`. `CoachingInput::validate()` runs last, and a failure is
`BuildError::Contract` (a builder bug). Database and decode failures are `BuildError::Db`.

### Field rules (TS M2 Task 26 plus R-1 to R-8)

| Field | Rule |
|---|---|
| evidence | Games with current-version metrics and their series points (version gate). Stale games are no evidence. |
| `platform`, `timeClass` | Of the account and the request (`contract_platform`, `From<chess_core::TimeClass>`). |
| `timeControl` | Most common `(base, increment)` of the evidence games; ties go to the smaller pair. `null` for daily or without clocks. |
| `band`, `rating` | R-4 with ruling Q2 (`player_band`: rating, else mean opponent rating, else **B1**). `rating` is the latest own rating, `null` when unknown. |
| `basis` | `analysedGames` = evidence games; `importedGames` = all games of the time class; window = first and last UTC dates of the evidence (R-5: both `now`'s date without games); `backfillComplete` = nothing of the time class is pending or analysing, and `select_for_analysis` (with the schedule job's status mapping, the beta plan, the precision stop) selects nothing more in it. |
| `areaScores` | All 7 in contract order. Recomputed (`compute_area_scores`) from the latest `last50` component snapshots, so `sufficient` and `needMore` are known. They equal the stored `area:*` rows. With no evidence games, no snapshot is used. |
| `metrics` | R-2. Catalogue order; rows with an area, a contract unit and a value. `value`, `low` and `high` are clamped to the unit range, and the interval is kept only with both ends finite and ordered. `bandMedian` and `bandPercentile` come from the band statistic (`value_adjusted`, else `value`, against the quartiles; `null` without a statistic or a direction). `sufficient = n ≥ min_n`. At most 80. |
| `weaknesses` | Estimates recomputed at `now` with the same code as build-profile, ranked by `rank_weaknesses`. The state and `P_imp` come from replaying the series. An estimate is claimed only if: `reason = ok` (R-3), `min_evidence_met`, `k_unweighted > 0` (T12 ruling), **`priority > 0`** (a measured excess over the band), and at least one exemplar (R3 T8). It is also dropped if its posterior lies outside its interval, which is never widened. Ranks run 1, 2, … over what remains. At most 24. |
| exemplars | From `exemplar_plies` of games within 6 months; ordered by ΔW, then recency, then game, then ply; one per game. Up to 5 for ranks 1–3 and 2 after. |
| `strengths` | R-1, `{area, label, metricKey, exemplars}`. Candidates are areas with a sufficient score ≥ 60, and weaknesses with `1 − confidence ≥ 0.8`. A weakness candidate needs the same evidence as a claim (`reason = ok`, minimum evidence, an event, a percentile), and is not also a ranked weakness. Each candidate needs at least one exemplar of its area's kind: tactics → `tactic_found`, calculation → `only_move_found`, middlegame → `conversion` or `defence`, endgame → `conversion` in the endgame phase. Openings, time and mindset have none. Sorted by percentile descending; at most 4 strengths, with 1–3 exemplars each. `metricKey` is set only when that metric is in `metrics`. |
| `openings` | `opening_lines` with a verdict (R: `null` verdict omitted) and at least one example (R3 T8). An example is, per game of the line, the fix spot's move, else the player's worst ΔW move in the first 30 plies; ordered by ΔW, then recency; up to 3 distinct games. The line key follows R-7 (`contract_line_key` re-applies Task 16's cut to any key over 40 slug characters). Names, ECO, SAN lists and alternatives are cut to their bounds. An alternative's `movesSan` is its single move. Ordered by games descending; at most 30. |
| `changes` | Onset rows of the history (the current replay) of the current detector version, with the onset in the last 90 days and its window and evidence (`p_change`, rates) present. Newest first; at most 20. `fromState null` becomes `not_an_issue` (R-7). `weaknessId` comes from `weakness_id_of_series`. The onset game is the platform game id. Causes are the stored cause hints. Up to 3 examples on each side of the onset come from the series' exemplars. A missed motif uses own-tactic exemplars whose best line has the motif; an allowed motif uses hung-material exemplars whose refutation has it. |

### `ExemplarPosition` (R-7, R-8)

- `gameId` is the platform game id. `fen` is the stored FEN before the move, and its side to
  move must match the ply's parity.
- SAN comes from replaying UCI on the FEN (`Position::push_uci`). `best` is `null` when the
  best move is the played move.
- `bestPv` and `refutationPv` keep their legal prefix (from `fen`, and from the position after
  `played`), cut to 16 plies. A `bestPv` that does not start with `best` is dropped.
- Tags are the `contract_tag()`s of the best and refutation tags, deduplicated, at most 12.
- Scores are White POV, read from `position_evals` at the game's analysis protocol. A final
  checkmate or stalemate after the move uses `evaluate_game`'s terminal score (±1000 or 0),
  which is not cached.
- A row without an eval, with an illegal move or with an over-long id gives no exemplar.

### Honesty rules

- Unknown is `None`: missing evals, intervals, percentiles and band statistics stay null.
- `n` goes with every rate: a metric's `n` is opportunities for rate rows.
- No NaN or infinity: every float is checked, and `validate()` rejects any that slips through.
- Intervals are never widened. Clamping to the unit range only narrows, and an item whose
  posterior lies outside its interval is dropped, not repaired.

### Coordinator rulings (Task 34 review)

1. **A weakness needs `priority > 0`.** An estimate whose mean is not above the band median
   has no excess. Ranking it would claim a weakness that the evidence does not show.
2. **A strength needs a claimable estimate.** That means `reason = ok`, the minimum evidence,
   and at least one event. There are no zero-event claims: a `NoEvents` estimate has its
   confidence forced to 0, so `1 − confidence` would otherwise "prove" a strength from nothing.
   A key that is a ranked weakness is never also a strength.
3. **`to_exemplar_position` returns `Option`.** A stored row that cannot give a valid position
   is skipped, never fabricated. Causes include an unknown eval, an illegal move, a FEN whose
   side to move disagrees with its ply, an over-long id, or an undecodable colour or ply.
   Skipped rows are counted in `BuildReport.skipped_exemplars` and logged. The next candidate
   (same order, another game) takes the slot.
4. **An alternative's `movesSan` is its single move.** If a stored PV line existed for an
   alternative, it would be used, cut to 16 plies. Task 18's `AlternativeMove` stores none, so
   today it is always the single move.

### T34 review, fix round 1

- **Strengths from weaknesses cite their own claim (review m2).** A weakness strength takes
  exemplars only of the kinds that show that weakness is not a problem (`weakness_kinds`):
  - W3.shallow, W3.deep and W4.missed → `tactic_found`;
  - W5 → `conversion`;
  - W6 → `defence`;
  - W8.basics and W8.advanced → `conversion` in the endgame phase.

  Every other key has no matching kind, so it is never a strength. This covers hung material,
  missed threats, allowed mates, vision errors, opening, time and positional keys. Its area's
  exemplars never stand in for it, for example a found only-move for "Few errors: Hung
  material". Area strengths keep the area kinds. Pinned by
  `weakness_strength_needs_exemplars_of_its_own_claim`.
- **`priority > 0` is pinned** by `below_median_weakness_is_never_ranked`: W1 at 0.04 against a
  band median of 0.08, with exemplars. Without the guard, it was ranked #1 at priority 0.
- **Metric percentile (review m3).** `bandPercentile` uses `value_adjusted`, and falls back to
  the raw `value` when no adjustment exists (no reference slope, or no game knowing both
  ratings). That is the same rule as the area scores (Task 15's `compute_area_scores`), so a
  metric and its area never disagree. No number is invented: the raw value is the measured
  one, only not opponent-adjusted.
- **Opening change keys (review m4).** An `opening:<line>` series key is mapped through
  `contract_line_key`, so a long legacy key still yields its change instead of being dropped. A
  history row that still cannot be stated is left out and logged.
- **Determinism (review m5).** The fix-spot candidate moves are read `ORDER BY game_id, ply`,
  so with a repeated position the earliest ply is chosen. Pinned by
  `same_data_and_now_give_the_same_input`.
- **`skipped_exemplars` counts distinct `(game, ply)` rows (review m6),** however many lists
  tried the row.
- **A past `now` (review m7).** Estimates, the 6-month exemplar window and the 90-day change
  window use `now`. The series replay (state and `P_imp`), `rating`, `basis`, `timeControl` and
  the snapshots use every stored row, including games after `now`. Live builds use the wall
  clock, so they are unaffected. A backdated build is therefore not a true snapshot at `now`.
  Making it one would need an `at`-bounded load everywhere, which is parked until something
  needs historical inputs.

### Snapshot time and `now`

`metrics` and `areaScores` come from the latest `last50` snapshots (the brief). Weaknesses,
strengths and changes are computed at `now`. The input therefore describes two moments:

- **`now`** is carried in the contract as `generatedAt`.
- **The snapshot time** cannot go in the contract. The M3 schema is strict
  (`additionalProperties: false`, zod parity), so a new field would break R3's parser.
  `build_coaching_input_report` instead returns a `BuildReport` with:
  - the input;
  - `snapshots_computed_at`: the newest `computed_at` of the snapshots used, or `None` when none
    was used;
  - `skipped_exemplars`.

  `build_coaching_input` returns only the input. Callers that show freshness (the CLI, the web,
  T35) use the report. The two moments differ only between a game's analysis and its follow-up
  build (the follow-up rule above), so the gap is short and visible.

## R3 hand-off

R3 depends only on the `coaching-input` crate (the types, `LIMITS`, `validate`, the schema). It
never depends on `profile` or the database. The builder is R2's; R3 calls it through the worker
or CLI (T35 wires `cli profile --json`).

## Consequences

- The builder re-estimates at `now`, so ages differ slightly from the last build's `started_at`.
  The states and rows are those of a replay of the same evidence.
- `metrics` and `areaScores` reflect the latest build-profile run (`snapshots_computed_at`). A
  game analysed after it shows up after the follow-up build.
- `serde_json`'s default number parser can be 1 ulp off when reading the builder's JSON back
  (the `float_roundtrip` feature is off). The text itself is shortest-round-trip, so
  `JSON.parse` reads it exactly. The round-trip test compares with a 1e-15 relative tolerance.
