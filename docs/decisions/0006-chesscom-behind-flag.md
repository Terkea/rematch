# 0006. chess.com integration behind a flag, with a Lichess-first fallback

Date: 2026-11-21
Status: Accepted

## Context

chess.com's User Agreement 4(D) may prohibit competing educational tools without written permission, while their API help page says commercial use is fine.

## Decision

Build the chess.com integration in full behind CHESSCOM_ENABLED. The owner requests written permission and OAuth access, and if permission isn't granted we launch Lichess-first. Until OAuth is approved, ownership is verified by a code in the profile location field.

## Alternatives considered

Ignore the risk; drop chess.com.

## Consequences

Launch scope depends on the owner's risk call.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
