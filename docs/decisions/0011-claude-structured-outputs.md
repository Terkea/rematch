# 0011. Claude API with structured outputs and a claim validator

Date: 2026-11-21
Status: Accepted

## Context

AI chess coaching is notorious for invented moves and facts.

## Decision

claude-sonnet-5-5 for reports, claude-haiku-5-5 for short explanations, and the Batch API for scheduled regenerations. Structured outputs via output_config.format. Every claim cites a game and ply, and a validator checks citations, moves and numbers against stored data. On failure, regenerate once, then drop the bad item.

## Alternatives considered

Free-form text; forced tool calls (rejected on Sonnet 5.5).

## Consequences

Reports are trustworthy and testable, at the cost of slightly higher latency.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
