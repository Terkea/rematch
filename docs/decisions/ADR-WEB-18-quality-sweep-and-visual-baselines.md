# ADR-WEB-18. Quality sweep, visual baselines and the throttle fix

Date: 2026-11-22
Status: Accepted (M4 Task 28)

## Context

M4 ends with a sweep: axe, sideways scroll and focus on the M4 routes, a cookie allow-list, and visual baselines. Two hand-offs came with it. The 4x CPU throttle check (ADR 0065) measured a trend step at 16.2 to 16.9 ms against a 16 ms budget on the production build, and the ruling was to make the step cheaper, not to move the budget. M5a T13 found that copy edits pass a `maxDiffPixelRatio: 0.001` capture.

## Decision

- **@axe-core/playwright 4.14.0** (MPL-2.0) is a web devDependency, as ruled: file-level copyleft, used unmodified, test-only, never shipped. axe-core resolves to 4.14.0 (4.14.1 is under the 7-day age floor).
- **`e2e/quality.spec.ts`** checks /styleguide, /sign-in, /sign-in/check-email, /welcome/age, /blocked, /app/accounts and /lab at 360px: no horizontal scroll, no serious or critical axe finding (wcag2a, wcag2aa), and Tab from the top reaches the main action with an outline. Two invited e2e users hold the signed-in states (`quality@`, who never answers the age question, and `quality-adult@`).
- **`e2e/cookies.spec.ts`** (tagged `@fakes`, it links chess.com through the fake): after sign-in and linking, every cookie is named `ci.` or `ci_` and is set for the app's own host.
- **`e2e/overlays.spec.ts`** closes the T8 hand-offs in a real browser: the drawer unmounts, the dialog only fades under reduced motion and the drawer travels in `--dur-instant`, the toast dismiss hits across its 44px band, and Ctrl+K stays shut in a textarea, a select and contenteditable text.
- **`e2e/visual.spec.ts`**: every `[data-section]` of /styleguide, plus /sign-in and /blocked, in the four visual projects, `animations: "disabled"`, `maxDiffPixelRatio: 0.001`, and also `maxDiffPixels: 40` (the stricter bound wins). Each capture pins its visible text in a `.txt` snapshot beside it, so a copy change fails even when it moves few pixels. `game-pages-visual.spec.ts` gets the same text snapshots and the same pixel bound.
- **Baselines are made against the production build** in the qa container, and the CI `e2e` job now runs the visual projects too. They pass on the dev server as well (the screenshot stylesheet hides its badge).
- **The throttle fix**, measured on the production build (trace of 20 steps at 4x): a step's extra main-thread work was about 14 ms of whole-page Layerize, 6 ms of Paint and 20 ms of script. Three changes, none visible:
  1. The roving tab stop moved from chart state to a small store read with `useSyncExternalStore`, so a key re-renders the two buttons whose stop changed, not the chart.
  2. Dot buttons are centred with `-ml-1.5 -mt-1.5` instead of a translate, which removes 500 transform nodes.
  3. The button overlay takes `will-change: transform` while it holds focus (`focus-within`), so a moved focus ring repaints that layer and skips the page-wide Layerize. This is a scoped exception to M9's "will-change only while animating", recorded in DESIGN.md Revisions.

## Consequences

- The budget stays at 16 ms. Key cost after the fix: median 0.5, 0.0 and 1.4 ms over three runs (p90 −9.9, 9.6, 6.4); before, 29.0 ms on the same host and 16.2 to 16.9 ms in T27.
- The `will-change` layer exists only while a dot has focus, so the idle page holds no extra layer.
- 136 baseline files (68 PNG, 68 text) plus the 12 regenerated game-page PNGs and their text. Any copy change now needs a baseline update in the qa container.
