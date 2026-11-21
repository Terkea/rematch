# 0007. Pinned analysis protocol and a shared position cache

Date: 2026-11-21
Status: Accepted

## Context

Metrics must be comparable over time and across users, and CPU use must be bounded.

## Decision

A pinned Stockfish version at about 300k nodes per position, deeper on critical positions. A FEN-keyed cache shared by all users, seeded from the CC0 Lichess eval dump. Games are re-analysed when the protocol changes. Every position is evaluated once, which gives the quality of both players' moves.

## Alternatives considered

Fixed depth; a fixed time per move.

## Consequences

Results are deterministic and cacheable. Upgrades need a re-analysis job.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
