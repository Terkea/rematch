# 0037. Own PGN parser with chess.js-compatible FEN and SAN

Date: 2026-11-21
Status: Accepted. Implements 0030.

## Context

The TypeScript stack uses chess.js 1.4.0 for PGN parsing, SAN, FEN and UCI. `fen_before` and `fen_key` are the eval-cache key, shared between the TS and Rust workers during the migration, so any byte of drift splits the cache and breaks parity tests. The usual Rust options, shakmaty and pgn-reader, are GPL-3.0 and are excluded by the licence allowlist (ADR 0031). cozy-chess (MIT) provides move generation only: no SAN, no PGN, and FEN and castling conventions that differ from chess.js.

## Decision

- We write our own thin layer over `cozy_chess::Board` in `chess-core` (`board.rs`, and the PGN parser in `pgn.rs`). No GPL code is read or copied; chess.js (BSD-2-Clause) is the behavioural reference.
- chess.js 1.4.0 byte-compatibility is a requirement, not a nicety:
  - **FEN printing.** The en-passant square is printed only when the side to move has a legal en-passant capture (a pinned capture prints `-`). The halfmove clock and move number are kept as plain integers, uncapped (cozy-chess caps the clock at 100).
  - **Repetition key.** chess.js hashes a *different* ep rule: the ep file counts whenever an enemy pawn stands next to a double-pushed pawn, legal or not. `Position::repetition_key` uses that rule (placement, side, castling, ep file), so threefold detection matches `isThreefoldRepetition`. The two rules are deliberately separate.
  - **UCI.** Castling is the king's two-square step (`e1g1`), converted from cozy-chess's king-takes-rook form. `play_uci` accepts both forms. Promotion is a lowercase suffix.
  - **SAN.** chess.js disambiguation (square if both a rank and a file clash, rank if only a file clashes, otherwise file), `x`, `=Q`, `+`/`#`. Parsing strips `[+#]?[?!]*`, accepts `0-0`, and falls back to chess.js's permissive parser (over-disambiguated forms, `e2e4`, `Pe2-e4`).
  - **Insufficient material.** Exactly chess.js's rule: K v K, three pieces with one bishop or one knight, or only kings and bishops on one square colour.
- Known, accepted differences: candidate moves are ordered by chess.js's square scan (from, then to) rather than its offset order, which can only matter for ambiguous permissive input; null moves (`--`) are not supported; FENs that cozy-chess considers structurally invalid (for example a castling right without its rook) are rejected, where chess.js loads some of them.

## Reference fixture

`backend/fixtures/board/chessjs-reference.json` was generated with chess.js 1.4.0 (`Chess#move` verbose output: `san`, `from`, `to`, `promotion`, `before`, `after`, plus the final `isCheckmate`, `isStalemate`, `isInsufficientMaterial`, `isThreefoldRepetition`). It holds the hand-picked lines of the plan (Scholar's mate, promotion captures, both castlings, en passant, pinned and unpinned double pushes, disambiguation, Sam Loyd's stalemate, threefold) and 12 seeded random games (about 1,200 moves) biased to captures, castling and promotions. `tests/board.rs` replays every move.

## Consequences

- Parity is checked by data, not by hope: regenerating the fixture with a newer chess.js shows any behavioural drift at once.
- We own the SAN and PGN code and its bugs. The fixture and the ported TS PGN tests are the safety net.
