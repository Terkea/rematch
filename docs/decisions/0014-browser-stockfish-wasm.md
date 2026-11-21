# 0014. Stockfish WASM in the browser, GPL-compliant

Date: 2026-11-21
Status: Accepted

## Context

Drills, retry and free single-game review need engine moves with low latency.

## Decision

Ship a pinned stockfish.wasm build as separate files, run it in a Web Worker over UCI, host the matching source and add a /licenses page.

## Alternatives considered

A server-side interactive engine (latency, cost); tablebase-only drills.

## Consequences

GPL obligations are limited to the Stockfish component.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
