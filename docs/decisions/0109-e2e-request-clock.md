# 0109. A pinned "today" for the visual e2e

Date: 2026-11-24. Status: accepted (lane `coachvis`, branch `fix/coach-visual`). Builds on ADR 0067.

## Context

From 24 Nov 2026 the `e2e` job on `main` failed on "the coach pages look as they did", in all four visual projects, on every pull request. Nothing in the UI had changed. The weakness, area and progress pages draw a weakness timeline that runs to today, and the server took today from the wall clock. The baselines were made on 23 Nov, so the next day the axis ended at "24 Nov" and every tick moved a few pixels (77 to 248 pixels). The text snapshots showed the same one-line change. CI only reported the weakness page because the test stops at its first failed capture.

There was a second wall-clock read. The coach seed left `weakness_estimates.updated_at` at its `now()` default, and the weakness timeline ends at the latest of its last segment, `updated_at` and today.

Playwright's `page.clock` cannot help: these pages are server components, and the date is read on the server.

## Decision

1. **`requestNow()`** (`apps/web/src/server/clock.ts`) is the request's current time. It is the wall clock unless `E2E_CLOCK=1` is set and the request carries a `ci-e2e-now` cookie with a valid date, in which case it is that date. The weakness, area and progress pages take their "now" from it. The weakness view takes it as a prop, so it no longer reads the clock while rendering.
2. **Only the e2e overlay sets `E2E_CLOCK=1`** (`docker-compose.e2e.yml`, so both the dev-server and the production e2e). Only the coach visual test sets the cookie, pinned to `2026-11-23T12:00:00Z`, the day its baselines show. Every other spec keeps the wall clock.
3. **The coach seed writes a fixed `updated_at`** (23 Nov 2026) on its estimates, like every other date in that seed.

## Options considered

1. **Regenerate the baselines** (rejected). They would fail again the next day.
2. **Mask the timeline in the capture** (rejected). It hides the part of the page most likely to regress.
3. **Shift the clock of the whole web process in e2e**, with a preloaded `Date` patch (rejected). Sessions, mail tokens and rate limits compare JavaScript time with Postgres `now()`, and the offset would grow every day.
4. **A cookie the server honours only under an e2e flag** (chosen). It is scoped to one browser context, so no other spec sees it.

## Consequences

- The coach visual baselines stay valid on every day. A page that later starts reading the wall clock in its render will fail the same way; the fix is to take `requestNow()` from the page.
- A production server honours the cookie only if an operator sets `E2E_CLOCK=1`, which nothing but the e2e overlay does. It changes only what these three pages treat as today.
