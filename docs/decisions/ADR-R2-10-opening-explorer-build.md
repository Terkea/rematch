# ADR-R2-10. Opening explorer build: one parallel pass, verified before activation

Date: 2026-11-22
Status: Accepted. This is a placeholder number; the controller renumbers R2 ADRs before merging. It builds on ADR-R2-9 (the streamed, verified, never-stored dump) and ADR-R2-23 (reference build storage), and carries TS M2 ADR 0035's parameters into the Rust backend.

## Context

The repertoire's "in theory" test (≥ 50 games and a ≥ 5 % share) and its opening verdicts need an opening explorer for each rating band and time class. That data comes from the Lichess monthly dump. A month is about 31 GB compressed and about 95M games, in time order. TS M2 (ADR 0035) read the dump in a single thread, after downloading it. R2 streams the dump instead (ADR-R2-9). Every byte must still be hashed before the result is trusted, so a build is one long transfer, and the parser has to keep up with it.

## Decision

### Parameters (from TS M2 ADR 0035)

- **One month per build**, built quarterly by the operator CLI (`lichess-db build --month M --explorer`).
- **Sample size about 2M games** (`max_games`). Each game counts only its **first 30 plies** (`max_ply`). The replay goes through `Position::play_san`, and the parser never replays the full PGN.
- **Prune** rows whose total is below **3** (`min_row_count`) before activation.
- **Bands by the mean rating** of the two players: `explorer_band = band_of((white + black) / 2)`.
- **Four time classes**: bullet, blitz, rapid and classical. Correspondence (`TimeControl "-"`) is excluded.
- Each row's key is `fen_key | band | time_class | uci`, and it holds White wins, draws and Black wins. Unfinished games (`*`) count nothing.

### Sampling across the whole month (T19 review ruling)

- The dump is in time order. Taking the first 2M games would sample only the first hours of the month, so the build samples by rate: `sample_rate = min(1, max_games / expected_games)`.
  - The sample uses the deterministic FNV hash of `Site` and the seed (ADR-R2-9).
  - `expected_games` comes from `--expected-games` if given. Otherwise it is the `games_scanned` of the newest active or superseded build that was not capped. With neither, it is `DEFAULT_EXPECTED_GAMES` = 100M, a rough 2026 figure.
- `max_games` is the **target**. The pipeline stops only at a **safety cap** of `max_games × 1.25`.
  - A build that reaches the cap is recorded with `params.capped = true`. Its count is never used as the next month's estimate.
  - With a correct estimate, the binomial spread at 2M is about ±0.1 %, so the cap is not reached.
- **BOT games are excluded**: either side has `WhiteTitle` or `BlackTitle` `"BOT"`.
- **Provisional-looking games are excluded** from the explorer, because a provisional rating would put a game in the wrong band. The dump has no provisional flag, so this is a heuristic:
  - A side looks provisional when `|RatingDiff| ≥ 25` in that game.
  - Under Glicko-2, an established rating (RD ≈ 60) moves about 10 points in an even game, and about 20 even in a large upset. A provisional rating (RD ≥ 110, the Lichess threshold) moves about 30 points in an even game.
  - A game with no `RatingDiff` tag is not flagged.
  - The heuristic lives in `headers::looks_provisional`. `SampleFilter::exclude_provisional` turns it on; it is off by default and on for the explorer.

### Single-pass parallel pipeline

`run_pipeline` (`pipeline.rs`) works in these steps:
1. The async reader pulls games from the dump stream. The network, SHA-256, zstd and the splitter all run on that one task.
2. It fills `Vec<String>` batches of `batch_games` games (2,000) into a bounded channel of `channel_batches` batches (2 × threads).
3. Each batch goes to a rayon pool of `threads` threads (default: every core). The pool runs `par_iter` over the batch: headers, then `keep`, then `GameSink::observe`, which for the explorer is the SAN replay into `(fen_key, uci)` pairs.
4. At most two batches are in flight. Their results come back **in batch order**, through `buffered`, not `buffer_unordered`.
5. `GameSink::merge` runs on the async side, one batch after another.

Memory is bounded by `channel_batches + 2` batches plus one flush interval of explorer keys.

### Deterministic merges

- Batches merge in source order, and the cap is applied in that order. So the kept games, the `games_seen` count at the cap and every explorer count are the same at any thread count.
- `pipeline_is_deterministic_across_thread_counts` compares 1 thread with 4, with 9-game batches and a flush every 37 games, and gets identical rows. Mutation check: with `buffer_unordered`, three ordering and cap tests fail.
- A panic in `observe` is caught and becomes an error; it does not abort the process.

### Activation only after verification (R-9)

The build runs in this order:
1. Fetch `sha256sums.txt`.
2. Create the `building` row, with its parameters in `params`.
3. Run the pipeline. Flush every `flush_every_games` (50,000) games, and once more at the end.
4. Drain the rest of the body through the hasher without decompressing it.
5. `ShaHandle::verify` checks the digest. `finish` is never used.
6. On a match:
   1. record `params.capped`;
   2. `set_source_sha`, with `games_scanned` = games seen;
   3. `prune_explorer`;
   4. `activate_build`, which supersedes the previous build and deletes its rows.

Any error after step 2 calls `fail_build`. That deletes the build's rows and leaves the previous active build untouched. Such errors include a transport error that could not be resumed, a checksum mismatch, a store error, and a month that is not newer.

`--verify none` skips steps 1, 4 and 5. It prunes and marks the build `unverified`, which is never served.

A build records one `job_metrics` row: `kind = "lichess-db:build"`, `total_ms`, and the stages `read_ms`, `parse_ms`, `aggregate_ms`, `flush_ms`, `drain_ms`, `prune_ms` and `activate_ms`.

### Flushes are not retried

`flush_explorer` adds counts, so retrying a chunk that may already have committed would count it twice (T17 review). The build never retries a flush. A failed flush fails the whole build, and since the stream cannot be replayed, the operator reruns the month. A double-counted chunk can therefore never reach an active build. Making chunks idempotent with a chunk marker would need a migration, and it would only pay off with in-process flush retries, which this design does not have.

### Transfer robustness (T19 review ruling)

- Before the body starts, a connect error, a timeout, a 5xx or a 429 is retried: 3 tries in total, with a doubling backoff from 2 s. A 4xx such as 404 is not retried.
- A transport error mid-body is resumed in-process. The resume is a GET with `Range: bytes=<read>-` and `If-Range: <strong ETag>`, and the answer must be a `206` whose `Content-Range` starts exactly at the offset. The resumed bytes go into the same hashing stream. There are at most 5 resumes.
- Without a strong ETag, or with a `200`, the error is final, and the hash stays incomplete, so `verify` can never pass.
- The zstd window is capped at 2^24 (`window_log_max`; Lichess uses 2^23), so a hostile frame cannot force a large allocation.

### Throughput

The measurements are in `docs/research/r2-lichess-db-benchmark.md`, taken on a 24-thread Ryzen 9 9900X:
- On synthetic data, games/s scales 5.9× from 1 to 24 threads, and parsing is the bound.
- On the real 2026-10 dump, the run is network-bound at 3.5 MB/s, about 10.8k games/s.

A full month is therefore set by its transfer, about 2.5 hours at that rate, not by the CPU. `lichess-db bench` is a CLI subcommand only. It is never part of the gate.

## Alternatives considered

- **Multiple passes, or a pre-scan to count the month.** That would download 31 GB twice. We use the last build's count instead.
- **First-N sampling.** It is biased to the start of the month (rejected by the T19 review).
- **`buffer_unordered` merges.** They are faster on a skewed batch, but the cap would no longer be deterministic, and nor would the results under a cap.
- **Bottom-k by hash (R-10)** gives an exact sample size without an estimate. It was kept for the reference-player selection (Task 21). For the explorer, a ±25 % miss on the sample size is harmless.

## Consequences

- An explorer build takes as long as its transfer. If a resume is impossible, a late network failure means a rerun of the month.
- The CLI `build` uses a 6-hour statement timeout, because the prune and the activation delete millions of rows.
- `--band-reference` is refused with a `Config` error until Task 21 wires it into `build`.
- `DumpError` gains `Store` (Postgres) and `Config` (invalid options).
