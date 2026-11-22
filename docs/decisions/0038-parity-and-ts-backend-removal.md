# 0038. Parity acceptance and removal of the TS backend

Date: 2026-11-22. Status: accepted (parity, R1 Task 24). R1 Task 27 adds the removal section.

## Context

ADR 0030 moves the whole backend to Rust. The TS worker is the reference, so it can only be removed once the Rust worker is shown to give the same analysis on the same games. Accuracy, judgements and the eval cache feed every later profile and coaching metric, so close enough is not good enough.

## Parity

### Method

- **The harness.** `backend/parity/run.sh` runs both workers against their own databases. It uses its own compose project, with no host ports, and never the default one.
- **The fixture set.** `parity seed` inserts the same fixture set into each database:
  - the recorded Lichess games, with their server evals;
  - the chess.com month;
  - PGN rows: mates, Sam Loyd's stalemate, threefold, a game without clocks, a chess960 start, and the `RUY` line.
- **Pending state.** Every game is `pending`, with `pending_since` an hour ago.
- **Enqueueing.**
  - TS: `apps/worker/scripts/enqueue-pending.ts` (Task 27 deletes it).
  - Rust: `parity seed --enqueue` or `parity enqueue-pending`.
  - `tools/parity` doesn't depend on the `analysis` crate (that would be a Cargo edit). So its `enqueue_analyse` repeats the options of `analysis::jobs::analyse::analyse_enqueue_options`: the same kind, the same dedupe key, the account as `user_key`, and 3 attempts.
- **The engine.** Both workers run the same checksum-pinned Stockfish 19 binary with the same settings: Threads 1, the default Hash, `ucinewgame` per position, and `go nodes 300000`. Every engine eval is therefore reproducible.
- **Phases.** The games are analysed in phases (`parity phases`): each Lichess-eval game alone, in fixture order, then all the rest.
  - The eval cache keeps the first write per position (ADR 0027).
  - A game that misses the cache uses its own Lichess or engine eval.
  - Without phases, a position shared with a Lichess-eval game gets whichever eval was written first. That depends on scheduling: the queue order and the pool size.
  - Phasing makes the cache state each game reads the same on both workers. It changes no tolerance.
- **The comparison.** `parity compare --a TS --b RS` loads both databases and keys them by `(platform, platform_game_id)`, `ply` and `(fen_key, protocol)`.
  - It prints every mismatch as `<platform_game_id> ply <n> <column>: <a> != <b>`. Game columns use `game` in place of `ply <n>`, and evals use `eval <fen_key> [<protocol>]`.
  - It exits 1 on any mismatch.

### Tolerances

| Table | Exact | Within |
|---|---|---|
| `games` | `analysis_status`, `analysis_protocol`; presence on both sides | `white_accuracy`, `black_accuracy`: 1e-6 |
| `moves` | the move count per game; `color`, `san`, `uci`, `fen_before`, `best_uci`, `judgement`, `phase`, `clock_before`, `time_spent`, `decided` | `win_before`, `win_after`, `delta_w`: 1e-9; `accuracy`: 1e-6 |
| `position_evals` | the key set; `score_cp`, `score_mate`, `best_uci`, `pv`, `source` | — |

### Result

**2026-11-22: `parity: 15 games, 558 moves, 436 evals compared, 0 mismatches`, exit 0.** A second run gave the same result. Details are in `docs/research/r1-parity.md`.

The first run showed two classes of difference:

1. **Shared positions (scheduling nondeterminism).** The opening plies differed in games that share positions with the Lichess-eval games. Both workers behave identically here, and the phases above remove the effect.
2. **Clockless games (a TS bug, fixed in TS).** TS reported the base time as `clock_before` on the first move of each colour in a game without clock comments.
   - Rust reports `NULL`. That was a deliberate T5 rule, and it is the one R2's time-trouble rule needs: a game without clocks is unknown, never "no trouble".
   - `packages/chess-core` `buildMoveRecords` now does the same, with a regression test.
   - Only clockless games are affected: chess.com daily games, and PGNs without `%clk`.

Task 25 covered live games. On 500 of the owner's chess.com games, the Rust worker matched TS exactly on accuracy (66.1) and blunders per game (3.13).

## Consequences

- The parity gate for Task 27 is met: exit 0 is recorded in `docs/research/r1-parity.md`.
- Parity can't detect behaviour that both workers share, such as the cache's first-write-wins rule (ADR 0027). Only the TS reference tests cover that.
- Rows that the TS worker wrote before the `clock_before` fix keep the old value on clockless games until they are re-analysed. chess.com live games always carry clocks, so only daily games are affected.
