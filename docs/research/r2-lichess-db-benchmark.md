# R2 lichess-db pipeline throughput (Task 20)

Date: 2026-11-22. Code: `backend/crates/lichess-db/src/{pipeline,bench}.rs` on branch `r2/t20`.

## Host

- CPU: AMD Ryzen 9 9900X, 12 cores and 24 threads. It runs under Docker Desktop on Windows 11
  (WSL2, Hyper-V), and the container sees all 24 logical CPUs.
- Build: `cargo build --locked --release -p lichess-db` inside the `rust-dev` container.
- Other work may have been running on the host at the same time (a parallel lane was active), so
  the numbers are indicative, not best-case.

## What is measured

`lichess-db bench` streams a compressed dump through the real chain (SHA-256, zstd with a 2^24
window cap, the splitter), then through `run_pipeline` (rayon `par_iter` over 2,000-game batches:
headers, `keep`, and a 30-ply SAN replay) into an in-memory `ExplorerSink`. Its flushes drop
the rows, so Postgres is **not** measured. The stage columns show busy time:
- `read` is time spent waiting on the stream: network, hash, zstd and split, on a single task.
- `parse` is the summed batch wall time on the pool. Up to two batches run at once, so it can
  exceed the elapsed time.
- `aggregate` is the in-order merge into the hash map, on the async side.

The stages overlap, so the largest one is the bottleneck ("max stage").

## Synthetic (no network)

Command: `lichess-db bench --synthetic 500000 --threads 1,4,0`

| threads | games | s | games/s | compressed MB/s | PGN MB/s | read ms | parse ms | aggregate ms | flush ms | max stage |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 1 | 500000 | 20.45 | 24452 | 0.1 | 21.7 | 1558 | 20316 | 2686 | 0 | parse |
| 4 | 500000 | 6.36 | 78658 | 0.2 | 69.8 | 1615 | 8291 | 1888 | 0 | parse |
| 24 | 500000 | 3.47 | 144193 | 0.3 | 128.0 | 1213 | 3676 | 2044 | 0 | parse |

- Games/s scales with threads: 3.2x at 4 threads and 5.9x at 24. Parse stays the largest stage.
  At 24 threads, the serial merge (2.0 s) and the reader (1.2 s) are close behind, so more cores
  would soon be bound by the single-task merge.
- **Caveat:** the five fixture games repeat, with only `Site` changed. They compress a few hundred to
  one, so the compressed MB/s is meaningless here. They also add few distinct explorer keys, so the
  merge is cheaper than on real data. Treat these rows as a ceiling for the parser.

The bench filter is not the build filter. It keeps every eligible game (rate 1.0, BOT and
correspondence excluded), and since fix round 1 the build also leaves provisional-looking games
in by default.

## Real dump (network, run once, not a test)

Command: `lichess-db bench --month 2026-10 --max-games 1000000 --threads 0`. It streams the
first ~1M games of `lichess_db_standard_rated_2026-10.pgn.zst` from database.lichess.org (about
330 MB compressed). It does not drain or verify, and it writes nothing to disk.

| threads | games | s | games/s | compressed MB/s | PGN MB/s | read ms | parse ms | aggregate ms | flush ms | max stage |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 24 | 1009984 | 93.48 | 10805 | 3.5 | 25.1 | 93440 | 9008 | 22876 | 0 | read |

- The run is **bound by the network**: `read` equals the elapsed time. 3.5 MB/s is what this host
  got from the mirror at that moment. On the synthetic dump, zstd and the splitter delivered
  about 128 MB/s of PGN. Real frames compress less and may decode at a different rate, which
  was not measured separately.
- 1,009,984 games were seen to keep 1,000,000. The other 9,984 were dropped by the filter (BOT,
  correspondence, abandoned or missing ratings); the bench does not break them down.
- On real games the merge costs 22.9 s per 1M kept games (about 44k games/s). That is the next
  ceiling after the network. A full build keeps only about 2M games, so this stays small next to
  the transfer.
- **What it means for a full build.** A 31 GB month at 3.5 MB/s takes about 2.5 hours, all of it
  transfer. Verification needs the whole file (R-9), so the transfer, not the CPU, sets the build
  time. The parser handles about 128 MB/s of PGN (synthetic) against about 25 MB/s arriving
  here, so header-parsing the whole month keeps up with the transfer.

## Not measured

- The Postgres flush: `flush_explorer` at 50,000-game intervals, the prune and the activation.
- Memory. T19 measured about 9 MiB peak for the stream alone; that was not re-measured with the
  pipeline. By construction the pipeline adds at most `channel_batches + 2` batches of 2,000
  games and one flush interval of explorer keys.
