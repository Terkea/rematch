# R1 parity: TS worker against Rust worker

R1 Task 24. Recorded 2026-11-22. Method and tolerances: ADR 0038, "Parity".

## Command

```sh
sh backend/parity/run.sh            # PARITY_PROJECT (default ci-parity), PARITY_LOGS, PARITY_TIMEOUT_S
```

The script uses its own compose project, with no host ports and a separate database per worker
(`chess_parity_ts`, `chess_parity_rs`). It does the following:

1. Migrates both databases with `cli db migrate`.
2. Seeds the fixture set with `parity seed`, phase by phase (`parity phases`).
3. Analyses each phase with both workers running at once:
   - TS: `scripts/enqueue-pending.ts`, then `src/main.ts`;
   - Rust: `rust-worker`, with its jobs from `parity seed --enqueue`.
4. Stops both workers with SIGTERM.
5. Runs `parity compare --a …/chess_parity_ts --b …/chess_parity_rs`.

## Fixture set

All games are synthetic or public fixture games from `backend/fixtures`. None of the owner's games are used.

| Source | Games |
|---|---|
| `lichess-games.ndjson`, with Lichess evals | 3 (the chess960 game is skipped by the importer) |
| `chesscom-month.json` | 5 (the chess960 game is skipped), including 1 daily game |
| PGN rows (Lichess-style, 180+0) | 7: `scholars-mate`, `no-clocks`, `chess960` (FEN start, skipped by both workers), Sam Loyd's `stalemate`, `fools-mate`, `threefold`, and the `RUY` line |

## Result (2026-11-22)

```
parity: 15 games, 558 moves, 436 evals compared, 0 mismatches
```

- Exit code 0, in about 110 s of analysis on a busy 24-thread host.
- A second run gave the same result.
- Both databases have 14 games `done` at `sf19-n300k-v1` and 1 `skipped`.
- Eval rows: 159 Lichess and 277 Stockfish.
- Moves: 413 with a clock, 116 with a judgement.
- Neither worker log has a warning or an error.

## Differences found, and how each was resolved

The first run gave 57 mismatches in two classes.

1. **Opening plies of games that share positions with the Lichess-eval games** (45 mismatches,
   in 5 games): `best_uci`, `win_*`, `delta_w`, `accuracy` and the game accuracies.
   - **Class: scheduling nondeterminism, the same in both workers.** Neither side has a bug.
   - **Cause.** The eval cache is shared, and it keeps the first write per position (ADR 0027).
     A game that misses the cache uses its own eval, Lichess or engine. The position after 1.e4
     was cached as `lichess:18` in both databases. Whether another game had read it before or
     after that write depended on scheduling. The TS worker enqueued in uuid order, the Rust
     worker in fixture order, and the two pools have different sizes.
   - **Fix: a harness change, not a tolerance.** The seed now runs in phases: each Lichess-eval
     game alone, then all the others. Every game then reads the same cache state on both
     workers. Engine evals need no ordering, because the same Stockfish with the same fixed
     search gives the same result.
2. **`clock_before` on the first move of each colour in games without clock comments** (12
   mismatches: the daily game, and five PGN rows without `%clk`).
   - **What differed.** TS gave the base time (180, or 86400 for the daily game). Rust gave
     `NULL`.
   - **Class: TS bug.** The Rust behaviour was a deliberate T5 review fix, so the TS worker
     changed, not Rust. Before, TS reported a clockless game as having clocks for two plies, so
     R2's `detect_time_trouble` would return "no trouble" (`Some(false)`) where the spec rule is
     "games without clocks are unknown, never no trouble".
   - **Fix.** `packages/chess-core/src/move-record.ts` now ignores the base when no ply has a
     clock. Its regression test is `clock before is null when clock config given but pgn has no
     clocks`, which has the same name as the Rust test.

## Not covered here

- **The owner's live games.** Task 25 analysed 500 of them with the Rust worker. Accuracy and
  blunders per game matched TS exactly (66.1, 3.13).
- **Tablebase probing.** Neither worker probes tablebases yet (R2 Task 23).
