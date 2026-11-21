# 0005. Better Auth for authentication

Date: 2026-11-21
Status: Accepted

## Context

We need email magic link, Google, and a custom Lichess OAuth (PKCE) provider.

## Decision

Better Auth, with Lichess as a custom OAuth provider and chess.com OAuth behind a flag.

## Alternatives considered

Auth.js/NextAuth v5 (still in beta, and now maintained by the Better Auth team); Clerk (cost, lock-in).

## Consequences

Auth data lives in our own Postgres.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
