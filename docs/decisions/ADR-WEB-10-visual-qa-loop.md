# ADR-WEB-10. Visual QA loop: Playwright, shots, lab and critique

Date: 2026-11-21
Status: Accepted

## Context

The owner requires a premium, non-generic UI. Tests cannot judge that, so every visual task needs screenshots looked at critically against `packages/ui/DESIGN.md`, repeatably and at the same sizes.

## Decision

- **Playwright projects** in `apps/web/playwright.config.ts`: `e2e` (Chromium 1280x800) and `visual-{390,1440}-{light,dark}` (390x844 and 1440x900, `colorScheme` set, `reducedMotion: "reduce"`). `baseURL` is `E2E_BASE_URL`, default `http://localhost:3100`.
- **`pnpm shots`** (`scripts/shots.ts`, pure helpers in `scripts/shots-lib.ts`) captures full-page PNGs for each route at 390 and 1440, light and dark, into `.shots/<slug>/<width>-<theme>.png`. It waits for `networkidle` and `document.fonts.ready`, hides `[data-lab-chrome]`, and exits 1 if any route answers 400 or more. `--motion` writes a 12-frame strip at 100 ms instead of using reduced motion. The script resolves Playwright from `apps/web`, so no root dependency was added. On Git Bash for Windows, set `MSYS_NO_PATHCONV=1` or `--routes /a` is rewritten into a Windows path.
- **Lab pattern**: `/lab/<surface>/<a|b|c>` pages wrap variants in `LabFrame`, which adds a variant switcher and a rationale footer, both marked `data-lab-chrome`. `/lab/layout.tsx` calls `notFound()` unless `labEnabled(process.env)` (`NODE_ENV !== "production" || LAB_ENABLED === "1"`) and sets `robots: { index: false }`. The layout is `force-dynamic` so the check runs per request.
- **Critique template** in `packages/ui/qa/critique-prompt.md`: a critic sees screenshots, DESIGN.md and REFERENCES.md, never source, and returns at most 10 ranked issues plus a verdict. Rounds are logged under `packages/ui/qa/log/`.
- **Playwright MCP** is configured in the project-level `.mcp.json` only (`@playwright/mcp@0.0.86`, isolated, 1440x900), never in user-level config.

## Alternatives

- Screenshot tests with pixel baselines: rejected; the goal is critical judgement, not regression.
- Playwright as a root dependency: rejected to keep the lockfile unchanged.

## Consequences

- The MCP server only appears in a new Claude Code session; verify with `/mcp` (if it lists no tools, run `npx clear-npx-cache` and retry). Not verified in the implementing session; see the task report.
- Production builds 404 on `/lab` unless `LAB_ENABLED=1`. The layout, the lab index and `/styleguide` all call the single `labEnabled` in `lab/lab-enabled.ts`, whose `flagOn` is also what `env.ts` uses.
- Warning: `docker-compose.yml` defaults `LAB_ENABLED=1`. That is for local development only; never carry it into a deployment, or the lab and styleguide become public.
