# Motion

Motion is a headline feature: rich, choreographed, physical, never generic. Full law, springs and choreographies: `packages/ui/DESIGN.md` "Motion" (principles `M1` to `M10`). Code: `@ci/ui/motion` (`packages/ui/src/motion/tokens.ts`). Use Motion only through `@ci/ui/motion`. GSAP and Lenis are landing-only and banned in the app (B12 family, enforced by `pnpm design:check`).

## What animates

- User-caused changes and the named orchestrated moments only (M1): the analysis reveal, piece lift, travel and landing, captures, the eval bar's swing, pen strokes drawing, the page turn, a button's pen-press, rolling KPI numbers, overlays entering.
- Pieces have mass (`settleFor`, `pieceWeight`): lift, travel on `settle`, squash on `land`; knights arc. The eval bar uses `swing`, and `swing-big` (visible overshoot) for a change of 0.3 win chance or more.
- Pen strokes draw by `pathLength` with `--ease-pen` (M2).

## Tokens

- Durations: `--dur-instant` 90ms (press, piece lift), `--dur-quick` 160ms (menus, toggles, retracting marks), `--dur-base` 240ms (panels, drawers), `--dur-draw-small` 240ms, `--dur-draw` 420ms (one board stroke), `--dur-reveal` 600ms, `--dur-turn` 520ms, `--dur-stream` 180ms, `--dur-soak` 900ms.
- Easings: `--ease-out` (UI responses), `--ease-pen`, `--ease-turn`, `--ease-soak`.
- Springs (no CSS form; `spring` in `@ci/ui/motion`): `settle`, `land`, `pen`, `swing`, `swing-big`, `page`, `tilt`. A new spring states its damping ratio and overshoot in DESIGN.md first.
- Stagger: `--stagger-tight` 40ms, `--stagger-base` 80ms, `--stagger-pen` 120ms (ring to arrow, nothing else).
- Overlay entry: `overlay` in `@ci/ui/motion` (mirrors `--enter-dialog-y`, `--enter-dialog-scale`, `--enter-float-scale`, `--offset-float`).
- In Tailwind: `duration-(--dur-quick)`, `ease-(--ease-out)`. In Motion: `dur`, `ease`, `spring`, `stagger`.

## What never animates

- No fade-up on scroll, nothing scroll-triggered, no parallax or scroll-jacking in the app (M4, B11).
- No hover lift or shadow: hover changes tone, never position (M5).
- No bounce without a physical cause (M6). No `filter`, `box-shadow`, `clip-path` or layout properties (M9): only `transform`, `opacity` and SVG `stroke-dashoffset`. At most 12 marks at once; `will-change` only while animating.
- Nothing blocks input (M7): any input, Skip or Escape jumps to the end state, finishing marks over `--dur-quick`.
- Anything over 1s runs in full only on the first view (stored per user on the server); later views condense to 600ms or less (M8), with a Skip control.

## Reduced motion

Under `prefers-reduced-motion: reduce` (M10) there are instant state changes with at most one opacity fade of `--dur-quick`: marks appear drawn, pieces jump, bars set, page turns swap, long sequences show their end. Read it with `useReducedMotion()` from `@ci/ui/motion`. Sound is unaffected (its own setting; see `useBoardSounds` in `chess-ui`). Visual QA shots run with reduced motion; `pnpm shots --motion` writes a frame strip for judging the motion itself.

## Tried and rejected

- A timid, fade-based reveal: the owner wants "huge", choreographed motion, and a cross-fade climax was rejected in two critiques.
- Near critically damped springs for everything: feels dead. Use the named springs with their stated overshoot.
- Running a long reveal on every visit: replays are condensed (M8).
- Rolling numbers from JavaScript before hydration (the page flashes final, hides, then reassembles).
- Hover lifts on rows and cards.
