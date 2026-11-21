# 0040. chess.com first; Lichess accounts deferred

Date: 2026-11-21. Status: accepted (owner direction).

## Context

The owner asked to focus on chess.com for the start and to defer Lichess. Both importers already exist (R1 T10, T11) and the resumable import job handles both (R1 T21). The open legal item from spec §14 still stands: chess.com's public API is read-only and unauthenticated, but commercial use needs chess.com's permission (owner action in §18).

## Decision

- **chess.com is the launch platform.** Every end-to-end path is built and proven on chess.com first:
  - username linking and location-code verification (M4 T19) come before Lichess OAuth (M4 T18);
  - the R1 live smoke (T25) and efficiency benchmark (T26) run on chess.com games, starting with the owner's account `mtsgambit`.
- **`CHESSCOM_ENABLED` defaults on in development and test.** Production still needs the owner's go-ahead after the chess.com permission question is settled.
- **Lichess *accounts* are deferred.** Covered: OAuth sign-in, Lichess linking, the Lichess live smoke, and Lichess-specific UI. The Lichess importer code stays in the tree and in its fixture tests, so it doesn't rot, but no new Lichess account work is scheduled.
- **The CC0 Lichess game database stays.** R2's `lichess-db` pipeline is reference data, not an account integration. It provides the rating-band peer references and the opening explorer for every user, chess.com players included. chess.com publishes no bulk dataset. Bands are mapped with `to_lichess_equivalent`.

## Consequences

- The first end-to-end milestone is a chess.com player: link, import, Stockfish analysis, profile, coaching.
- Rating comparisons for chess.com players go through the band mapping, so its accuracy matters early.
- Re-enabling Lichess accounts later is a scheduling change, not a rewrite.
