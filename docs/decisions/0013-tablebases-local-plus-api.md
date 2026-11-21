# 0013. Local 5-piece Syzygy plus the Lichess tablebase API

Date: 2026-11-21
Status: Accepted

## Context

Endgame KPIs need tablebase-verified errors in bulk, and drills need exact play.

## Decision

Local 5-piece Syzygy files (about 1 GB) via Stockfish's SyzygyPath for bulk checks; the Lichess tablebase API for 6-7 piece interactive positions.

## Alternatives considered

The API only; the full 7-piece set (about 17 TB).

## Consequences

Small storage footprint. Bulk checks with 6-7 pieces are not exact.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
