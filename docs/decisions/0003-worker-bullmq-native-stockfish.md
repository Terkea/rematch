# 0003. Background workers on BullMQ with a native Stockfish 19 pool

Date: 2026-11-21
Status: Accepted

## Context

Analysing hundreds or thousands of games per user is CPU-heavy and long-running.

## Decision

Node workers on BullMQ and Redis. Stockfish 19 native binaries, one per core, driven by our own thin UCI wrapper with FEN validation and supervisor restarts.

## Alternatives considered

Analysis in the browser only; the existing Node UCI wrappers (stale); a managed cloud-eval API.

## Consequences

Needs hosting with dedicated CPU. The position cache keeps cost down.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
