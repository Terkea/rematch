# ADR-R2-12. Local Syzygy probe through a separate Stockfish set

Date: 2026-11-21
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs before merging. Rust port of TS M2's ADR 0032 (Task 20), done as R2 Task 23. Uses the tables fetched under ADR-R2-22.

## Context

Enrichment needs exact win/draw/loss for endgame positions (`RawMoveFeatures.tb_wdl_before/after`). The 3-4-5 Syzygy set (145 WDL + 145 DTZ files) is in `$DATA_DIR/syzygy`. The main Rust Syzygy reader, `shakmaty-syzygy`, is GPL-3.0, and the alternatives we know of wrap C (Fathom) through FFI; the workspace forbids copyleft crates and new dependencies need the coordinator. No full survey of crates.io was done. Stockfish already reads Syzygy tables and is in every image.

## Decision

- **Probe through Stockfish.** `enrich::tablebase::pool::SyzygyPool` starts its own `UciEngine`s (default 1) with `setoption name SyzygyPath`, and searches 1000 nodes, MultiPV 1, with default Threads and Hash. It never uses R1's `EnginePool`, which never sets `SyzygyPath`, so `sf19-n300k-v1` evals are unchanged (pinned by the test `main_pool_never_sets_syzygy_path`).
- **Score parsing** (`wdl_from_lines`): with the root in the tables Stockfish reports a tablebase win as `cp 20000 - plies` (loss `cp -20000 + plies`), a draw as `cp 0`, and a mate found by search as `mate n`. `mate > 0` or `cp >= 19000` is 1; `mate <= 0` or `cp <= -19000` is -1; any other cp is 0; no line is `None`. WDL is from the side to move's point of view.
- **5 pieces locally, never a guess.** A probe answers `None` without an engine call when the FEN is invalid, has more than 5 pieces, has castling rights (Syzygy has no castling and Stockfish would then search instead of probing), or when no `.rtbw` file exists for its material (either side first, e.g. `KBNvKQ`). Bare kings are a draw without an engine call. Positions with a legal en-passant capture are probed; the tables handle en passant (tested: the same position is a draw with `d6` and a loss without it).
- **Clocks reset.** The pool sends the position with halfmove clock 0 and fullmove 1, so the answer depends only on `fen_key` and can be cached. The 50-move rule is still applied from that position through the DTZ tables: a win that needs more than 100 plies to zero (a cursed win) is reported as a small cp and therefore read as a draw (0). The real halfmove clock of a game is not used; in amateur games a 5-piece ending already 50 moves without a capture or pawn move is rare, and caching per clock would multiply the rows.
- **No tables, no answers.** A directory without `.rtbw` files (or no directory) starts no engine, logs one `tracing::warn!`, and answers `None` for every probe. Engine crashes and timeouts are retried once on a fresh engine and otherwise answer `None`.
- **Cache.** `CachedTbProbe` reads and writes `position_tb` (`fen_key` of the canonical FEN, so an impossible en-passant square does not split keys; `source = 'syzygy5'`). Only `Some` answers are stored; uncovered positions touch neither the table nor the engine.
- **6 and 7 pieces** are not probed locally (the 6-piece set is about 150 GB). They may come from the Lichess tablebase API, only for drills (M6), never in the profile pipeline.
- **Manifest trust** is as ADR-R2-22: the files are checked against the mirror's 2024-06-23 SHA-256 listing before use.

## Verification

Expected values were checked once against `https://tablebase.lichess.ovh/standard` on 2026-11-21, then asserted with the real tables in Docker (`tests/tablebase.rs`): KPvK `8/4k3/8/4K3/4P3/8/8/8` draw with White to move and loss with Black to move; KQvK `8/8/8/8/8/4k3/8/KQ6 w` win; KRvKR `8/8/8/4k3/8/8/r7/4K2R w` draw; KBNvK `7k/8/8/8/8/8/8/KBN5 w` win with dtz 57; `K7/8/8/3pPk2/8/8/8/8 w` draw with en passant `d6` and loss without it; checkmate -1; stalemate 0. The DTZ value is checked through the 50-move rule: with Stockfish's root rule (win only when dtz + halfmove clock <= 99), the KBNvK position gives `cp 20000` at halfmove 35 and `cp 25` (cursed) at halfmove 60.

## Consequences

- No new crate and no FFI. The cost is one Stockfish process and a 1000-node search per uncached probe. The search itself takes milliseconds. However, `UciEngine::analyse` sends `ucinewgame` before every search, and Stockfish then re-reads the table directory. On the Windows dev bind mount that took about 9 s per probe, so the probe timeout is 30 s. Skipping `ucinewgame` for this pool would need an `engine` crate option (a follow-up).
- The probe API is WDL only. DTZ is used inside Stockfish but not exposed; a later need for DTZ values (for drills) would need a permissive Syzygy reader or the API.
- WDL depends on Stockfish's tablebase reporting staying as it is in Stockfish 19; the real-table tests catch a change.
