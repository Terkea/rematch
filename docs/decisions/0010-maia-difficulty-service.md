# 0010. Maia-2 as a separate Python service, with a fallback

Date: 2026-11-21
Status: Accepted

## Context

Error rates depend on how difficult a position is. Maia also enables 'typical for your rating' tags and human-like sparring.

## Decision

A services/maia Python container, called by the workers and the drills. Its licence must be checked before use; until then a MultiPV-spread difficulty proxy is used.

## Alternatives considered

No difficulty adjustment.

## Consequences

Adds a Python runtime to operate.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
