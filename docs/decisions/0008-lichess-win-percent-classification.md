# 0008. Lichess win% formulas for accuracy and move classification

Date: 2026-11-21
Status: Accepted

## Context

We need move-quality labels that are documented and defensible.

## Decision

Use the exact Lichess win%, accuracy and inaccuracy/mistake/blunder formulas (see docs/research/improvement-engine.md). Do not imitate chess.com's proprietary formula.

## Alternatives considered

Raw centipawn thresholds; chess.com-style labels.

## Consequences

Labels behave sensibly in won and lost positions.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
