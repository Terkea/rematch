# 0017. Per-game metrics plus append-only KPI snapshots

Date: 2026-11-21
Status: Accepted

## Context

Progress must be plotted over time with honest uncertainty.

## Decision

A game_metrics row per analysed game. kpi_snapshot rows for rolling last-N and weekly windows, each with its sample size and confidence interval, opponent-adjusted. An events table for chart annotations.

## Alternatives considered

Recompute on read; store only current values.

## Consequences

Charts are cheap to draw and full history is kept.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
