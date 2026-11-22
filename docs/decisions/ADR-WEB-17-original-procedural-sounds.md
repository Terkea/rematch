# ADR-WEB-17. Original procedural board sounds

Date: 2026-11-22
Status: Accepted (Task 12b)

## Context

The owner asked for piece sounds. Sounds from other chess products are off limits, and sample libraries bring licence and asset-provenance work.

## Decision

- Every sound is synthesised with the Web Audio API in `packages/ui/src/audio/`. There are no audio assets, so there is no third-party licence to track. Everything is original code, under the project licence.
- One shared engine and one AudioContext, created lazily and unlocked on the first pointer or key gesture. `play` does nothing while locked or muted, and at most one sound plays per 40 ms.
- Mute and volume (default 0.6) persist in `localStorage` (`ci.sound`), wrapped in try/catch. The pen sound is off by default. Reduced motion does not mute sound.
- `soundForMove` maps a chess.js move to a sound: game end, check, promotion, castle, capture, move.
- The engine is not wired into the board yet. Consumers call `useBoardSounds().play(soundForMove(move))` from their move handlers.

## Consequences

- Parameters live in code and DESIGN.md. They were not tuned by ear in this task, so the owner should listen in `/styleguide` and ask for changes.
