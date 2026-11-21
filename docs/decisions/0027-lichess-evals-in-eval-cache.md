# 0027. Lichess server evals accepted into the eval cache

Date: 2026-11-21
Status: Accepted

## Context

Lichess exports server-side evaluations for analysed games. Re-running Stockfish for those positions wastes the CPU budget that tiered analysis (ADR 0025) tries to protect. The evals come from a different engine build and search budget than our `ANALYSIS_PROTOCOL`.

## Decision

`evaluateGame` uses complete Lichess evals directly (the engine is called at most once, for the start position, which Lichess does not report) and writes them to the eval cache with `source: "lichess"` under the same protocol id as our own evals. Resolution order per position is: terminal rule, cache, Lichess eval, engine. Existing cache rows are never overwritten. Terminal final positions (checkmate, stalemate, other draws) are never sent to the engine or cached: they get a fixed white-POV score of +1000, -1000 or 0.

Note: the task brief calls this ADR 0023, but 0023 was already taken by the local-stack ports decision, so it is numbered 0027.

## Alternatives considered

A separate protocol id for Lichess evals (cache never shared, so most of the saving is lost); always re-analysing with Stockfish (cost).

## Consequences

Small score differences between Lichess and local evals can appear in the same cache. The `source` column keeps them distinguishable if we later need to re-analyse.

See ADR 0025.
