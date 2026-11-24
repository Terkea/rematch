# ADR-WEB-20. The styleguide's off-screen sections skip rendering

Date: 2026-11-23
Status: Accepted (fix/ci-green)

## Context

The 4x CPU throttle check in `e2e/charts.spec.ts` (ADR 0065, ADR-WEB-18) failed on the production build: a key step on the styleguide's 500-game trend cost 17.6 to 17.9 ms against a 16 ms budget, and passed 1 run in 5. The suspects were the recent brand changes: the palette, the pencil grain, the crayon and the logo.

We measured both commits in the qa container (production build, 4x throttle, 5 runs of 40 steps each):

- main 8ff1f07: median key cost 6.4 to 15.2 ms.
- 26538aa, before any brand work: 10.3 to 16.8 ms, and `charts.spec` failed at 16.5 ms. The brand work is not the cause.
- Removing the pencil textures, the brand section or every animation changed nothing.
- Keeping only the charts section (2.6k of the page's 18.6k elements) cut the cost to 0.0 ms on both commits.
- A trace showed Layerize (`PaintArtifactCompositor::Update`) at about 11 ms per frame for the whole 23,000 px page: 2.7 s of 4.0 s of main-thread time. A frame that also runs the step's render and paint misses its vsync, which costs a whole frame (about 16.7 ms). That is why the results are either about 0 or over 16.

The page kept growing (sections, rows, demos) after ADR-WEB-18 brought the step under budget. No single commit pushed it over the line.

## Decision

- Every direct `section` of the styleguide's `main` except `charts` gets `content-visibility: auto` and `contain-intrinsic-size: auto 100vh`. The charts section stays laid out. The trend measures its parent on load, and a skipped section has no size, so its dots would only appear once someone scrolled to it, and `charts.spec`, `quality.spec` and `visual.spec` wait for those dots before they scroll. An off-screen section skips style, layout, paint and layerize. `auto` remembers a section's real height after its first render, so the scrollbar stops changing once a section has been seen. One viewport is only the first guess. The styleguide is a lab page (LAB_ENABLED), so no user-facing route changes.
- The budget stays at 16 ms. It is right: the check measures one step on the chart, and the cost came from the rest of the page.
- `/styleguide?render=all` lays out every section, as the page did before. A skipped section holds its transitions and animations, so the specs that inspect the whole page at once use it: `visual.spec` (its tall viewport would otherwise grow with each unrendered section's `100vh` guess, past Chromium's 10,000,000 px limit), `quality.spec` (axe read a half-faded row as a contrast failure), `styleguide-states.spec` (a forced hover's transition never ran) and `touch-targets.spec`. `charts.spec` and `overlays.spec` use the plain page.

## Consequences

- The trend step's cost no longer depends on how big the styleguide gets.
- A section's placeholder height can differ from its real height until it renders, so a spec that measures or inspects the whole page uses `?render=all`.

## Also: the footer hides while a route loads

`review-reveal.spec` failed on GitHub and locally with a layout shift of 0.0915 against < 0.02. The logo work (04fd048) added the signed-in footer to `(app)/layout.tsx`. The layout is `min-h-dvh` with `main` at `flex-1`, so while a route's short loading skeleton streams in, the footer sits at the foot of the first screen. The page that replaces the skeleton pushes it off: a 0.0827 shift on a 390 px phone, from the footer alone. Each route's `loading.tsx` root now carries `data-route-loading`, and the layout hides its footer (`visibility: hidden`) while that is present. A hidden element's move is not a layout shift, and the footer shows where the page leaves it. Afterwards the reveal's CLS is under 0.01.
