# ADR-R2-9. Lichess database pipeline: streamed, verified, never stored

Date: 2026-11-21
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs before merging. Builds on ADR-R2-1 (crates and data layout) and replaces, for the Rust backend, TS M2's ADR 0034 (download to `data/`, then verify).

## Context

Coaching compares a player with peers at the same rating. The peer data (rating-band references, R2 Tasks 21 and 32, and the opening explorer, Task 20) comes from the CC0 Lichess monthly database. A recent month is large. These are the `Content-Length` values on 2026-11-21:

| file | compressed |
|---|---|
| `lichess_db_standard_rated_2013-01.pgn.zst` | 17.8 MB |
| `lichess_db_standard_rated_2020-06.pgn.zst` | 21.1 GB |
| `lichess_db_standard_rated_2026-10.pgn.zst` | 31.0 GB |

A month expands to several hundred GB of PGN. TS M2 (ADR 0034) downloaded the file into `data/`, verified it, then parsed it. That needs 31 GB of disk per month for a file that is read once and then thrown away.

We checked the format facts below on 2026-11-21 with 32-byte HTTP range requests piped to `od`, so nothing was stored:
- Each file is **pzstd** output. Every zstd frame is preceded by a skippable frame (magic `0x184D2A50`, a 4-byte payload holding the next frame's compressed size).
- The frame header sets window log 23 (8 MiB) and the content-checksum flag.
- `<base>/standard/sha256sums.txt` uses the `sha256sum` format (`<hex>  <file>`). It is about 18 KB, one line per month (166 lines).

We also ran a one-off smoke check, which is not a test. It streamed the first 2 MiB of the 2013-01 and 2026-10 files through this code, with nothing written to disk:

| | 2013-01 | 2026-10 |
|---|---|---|
| games split | 14,010 | 6,211 |
| headers parsed | 14,010 (0 malformed) | 6,211 (0 malformed) |
| with clocks | 0 | 6,190 |
| `movetext_sans` vs R1 `parse_pgn`, first 2,000 games | 1,833 identical; 167 differ only by over-disambiguated tokens (`Rd1d2` vs `R1d2`) | 2,000 identical |

In both files the cut range ended mid-frame, as expected, with a `Decode` error ("zstd stream did not finish"). Recent dumps write `Site` as `https://lichess.org/game/<id>`, and the game id is the last path segment in both forms.

## Decision

### The dump is never stored

`lichess-db` streams one month per run. The bytes flow:

`reqwest` `bytes_stream` → SHA-256 pass-through → `tokio_util::io::StreamReader` → `async_compression` `ZstdDecoder` → line splitter → one `String` per game.

- Nothing is written to disk: not to `DATA_DIR`, not to the working directory, not to a temp file. The test `nothing_is_written_to_disk` runs the CLI with all three pointing at an empty directory and checks that it stays empty.
- The month (`--month YYYY-MM`) and the mirror (`--base-url`, default `https://database.lichess.org`) are configurable. One run reads exactly one month.
- Multi-member decoding is enabled (`multiple_members(true)`). Without it, the decoder stops after the first skippable frame of a pzstd file and yields nothing. Test: `pzstd_layout_with_skippable_frames_is_decoded`.
- The decoder's default `window_log_max` (2^27) covers the 8 MiB window. We do not raise it, so a hostile frame cannot ask for more.

### Streamed SHA-256, verified against the whole file (R-9)

- The SHA-256 covers the **compressed** bytes as they arrive, so the result is the digest `sha256sums.txt` lists. `fetch_expected_sha` reads that file (capped at 1 MiB) and picks the line for this month's file name. A missing or malformed line is `SumsMissing`.
- Verification is whole-file. With `--max-games`, parsing stops, but `drain_to_end` pulls the rest of the body through the hasher. It does not decompress, so draining costs only the network and the hash.
- `ShaHandle::verify` returns the digest or `ChecksumMismatch { expected, actual }`. A body that ended with a transport error never counts as complete, so it cannot be verified.
- A mismatch is only visible after the last byte of a 31 GB stream. So every consumer writes into a `building` build and activates it only after `verify` succeeds (Task 20 `sha_mismatch_marks_build_failed_and_keeps_previous_active`). A partial build is never active. `--verify none` exists only for development, and its builds are `unverified`, never `active`.

### Bounded memory

- The chain is pull-based: no byte is read before the consumer asks for the next game.
- Each stage holds at most one network chunk or one 64 KiB decode buffer, plus the zstd window (8 MiB for Lichess files).
- A game longer than `MAX_GAME_BYTES` (1 MiB) is a `Decode` error, not a growing buffer. Lichess games are a few KB even with `[%eval]` and `[%clk]` on every ply.
- Proof: `bounded_memory_on_a_large_synthetic_stream` streams 200 frames, about 85 MB compressed and 200 MiB of PGN. It checks two things:
  - the backpressure invariant: the reader is never more than 1 MiB past the frame of the game just returned;
  - peak RSS (`VmHWM`) below 48 MiB. The measured peak is about 9 MiB in a debug build.
- Mutation check: we made the implementation buffer the body. The backpressure check then failed at game 1 (85 MB pulled, against a 1.5 MB bound). With the backpressure check disabled, the RSS check failed on its own (90 MiB).

### Splitting, headers, sampling and movetext

All of this is our own code, written from the PGN format and the observed dump layout. Nothing is taken from the `lichess-org/database` scripts.

- **Split:** a new game starts at a line beginning `[Event ` that follows a blank line.
- **Headers:** `parse_dump_headers` reads the following.
  - `Site`, and the game id from its last path segment.
  - Players, Elo (`?` becomes `None`), `TimeControl` (`base+inc`, or `-`), `Result`, `Termination`, and `UTCDate` + `UTCTime` as an `OffsetDateTime`.
  - `has_clock` and `has_eval`, from the movetext only.
  - When a tag repeats, the last one wins, as in `chess_core::pgn`.
- **Sampling:** a game is skipped if any of these holds:
  - `TimeControl "-"`;
  - `Termination` is `Abandoned` or `Rules infraction`;
  - an Elo is missing;
  - optionally, it has no clock, or its time class is not wanted.

  Otherwise it is kept when `fnv1a32(site ‖ decimal(seed)) / 2^32 < sample_rate`. The same seed keeps the same games on every run and every machine. Time classes come from R1's `classify_time_control`.
- **Movetext:** `movetext_sans` returns the first `max_ply` main-line SAN tokens without replaying them. Task 20 replays only those. It agrees with R1's `parse_pgn` on every fixture ply.

### HTTP client and errors

- **User-Agent:** always the fixed `chess-improver/0.1 (+https://github.com/chess-improver)` (ADR 0043). No request carries an email address, and nothing is read from `CONTACT_EMAIL`.
- **Timeouts:** connect 30 s, and at most 120 s of silence between reads. There is no total timeout.
- **Errors:** `Http(status)`, `Transport` (connection, TLS, timeout or a body cut short), `Decode` (bad zstd, an oversized game, a malformed header), `ChecksumMismatch`, `SumsMissing`.
- **Resume:** superseded by ADR-R2-10 (Task 20). A mid-body transport error is resumed with HTTP `Range` and `If-Range`, within a bounded budget.

### Hashed usernames, per-build salt

Dump usernames are never stored in clear. Reference selection (Task 21) stores only `sha256(salt ‖ lower(name))`. The salt is 32 random bytes, generated per build and held only in memory, so it is never written to the database, logs or disk. Hashes from different builds cannot be joined, and since the dump is not kept either, nothing remains from which to recompute them.

### Operator CLI, quarterly (R-11)

`lichess-db` is an operator CLI, not a worker job. Its subcommands:
- `scan` (this task): a smoke tool that prints games seen and kept and the verified digest;
- `build` and `bench` (Task 20);
- `syzygy` (Task 22).

It runs by hand or from cron **once a quarter**, on the latest complete month, for example `0 3 8 1,4,7,10 *` with `--month` set to the previous month. Lichess publishes month M in the first days of M+1. Reference data changes slowly, so quarterly builds keep it current without a 31 GB transfer every month.

## Alternatives

- **Download to `data/`, then verify and parse (TS M2 ADR 0034).** Rejected: it needs 31 GB of disk and twice the I/O, for data used once.
- **Verify before parsing (download twice, or keep a copy).** Rejected: it doubles the 31 GB transfer, or brings the disk back.
- **Trust TLS and skip the checksum.** Rejected: TLS does not catch a corrupt or stale file on the mirror, or a body cut short between frames.
- **Stop the download at `--max-games`.** Rejected (R-9): a partial file cannot be verified, so its data would be unverified.
- **Raise `window_log_max` to 31 "just in case".** Rejected: Lichess frames use 8 MiB, and a higher cap would only let a bad frame allocate up to 2 GiB.
- **pgn-reader or shakmaty for PGN.** Rejected: GPL. The `lichess-org/database` scripts are not used either, because of their licence.

## Consequences

- Every build transfers a whole month, even for a small sample. On a typical connection that takes tens of minutes to a few hours. `scan` reads the whole file too.
- Consumers must treat their output as provisional until `verify` succeeds (Task 20 does).
- A network failure late in the stream wastes the run. A possible follow-up is HTTP `Range` resume within one process: the hasher state stays in memory, so the digest could continue.
- Tests need no network. They compress a 5-game fixture (`backend/fixtures/lichess-db/sample.pgn`, 4.4 KB) in the test and serve it with wiremock or a raw local HTTP server.
