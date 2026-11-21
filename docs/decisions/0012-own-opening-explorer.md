# 0012. Build our own rating-band opening explorer

Date: 2026-11-21
Status: Accepted

## Context

The Lichess Explorer now needs a user token and allows about 20 requests per minute per user. chess.com-only users have no token at all.

## Decision

Aggregate the CC0 Lichess game database into FEN-to-move statistics by rating band and time control, store it in Postgres and refresh it quarterly. Use the live explorer only for occasional lookups with the user's own token.

## Alternatives considered

The live explorer at scale; self-hosting lila-openingexplorer (AGPL).

## Consequences

A one-off data pipeline and some storage cost, with no external rate limit.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
