# 0015. UI stack and an anti-slop design process

Date: 2026-11-21
Status: Accepted

## Context

The owner wants a premium, animated, non-generic UI.

## Decision

The Annotated-scoresheet direction. Tailwind 4 tokens; shadcn 4 as a source only, with Base UI; Motion 14 (GSAP and Lenis on the landing page only); visx charts; react-chessboard (MIT); Phosphor icons. Process: DESIGN.md and a ban list first, a /styleguide page, three mockups per major surface on /lab routes, Playwright screenshots, critique by a fresh subagent plus Impeccable, two to three passes, then visual regression baselines.

## Alternatives considered

Stock shadcn; Aceternity or Magic UI; chessground (GPL).

## Consequences

Slower UI iteration, and a much better result.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
