# Web Foundation and Design System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A running Next.js app in Docker Compose with sign-in (email magic link, Google, Lichess), chess.com linking with location-code verification, the age gate, and a live event stream. Alongside it, the "Annotated scoresheet" design system in `packages/ui` (DESIGN.md, tokens, board with red-pen annotations, charts), shown on `/styleguide` and `/lab`, and checked by a screenshot-and-critique loop that ends in visual regression baselines.

**Architecture:** `apps/web` is a Next 16 App Router app. Better Auth stores users, sessions and OAuth accounts in Postgres through Drizzle (`@ci/db`). A join table links users to the existing `linked_accounts` rows, so the worker's data model stays as it is. `packages/ui` is a source-only component package, consumed through per-folder subpath exports and themed by Tailwind 4 `@theme` tokens. `/api/events` streams the M1 `user_events` rows over SSE: it subscribes to Redis first, then replays from Postgres. A client hook patches TanStack Query caches, with polling as the fallback. Playwright, `scripts/shots.ts` and a fresh-subagent critique make up the design QA loop.

**Tech Stack:** Next 16.4.6, React 19.3.1, Tailwind 4.3.5, Motion 14.14.0, better-auth 1.7.27, Drizzle 0.45, ioredis 6, @tanstack/react-query 5.x, react-chessboard 5.12.1, chess.js 1.4.0, @visx/visx 4.0.0, @number-flow/react 0.6.2, sonner 2.0.8, vaul 1.1.2, cmdk 1.1.1, next-themes 0.4.6, @phosphor-icons/react 2.1.10, shadcn CLI 4.23.0 with @base-ui/react 1.9.0, nodemailer, Mailpit, @playwright/test 1.65.1, @playwright/mcp 0.0.86, @axe-core/playwright, Vitest 5 with @testing-library/react and jsdom.

**Spec:** `docs/superpowers/specs/2026-11-21-chess-improver-design.md`: §6, §7, §11, §13a (client and delivery), §14 (age gate, cookies), §18. Also `docs/research/ui-tooling.md` §6–7, `docs/research/technical-facts.md` §1–2 and §6, and the frontend-design skill (`frontend-design:frontend-design`). Roadmap row 4: `docs/superpowers/plans/2026-11-21-roadmap.md`.

**Precondition:** Milestone 1 has landed, including Task 12b. That means `userEvents` (`id bigserial`, `accountId`, `type`, `payload`, `createdAt`) is exported from `@ci/db`, and the worker's `publishEvent` publishes JSON `{ id, type, payload }` on `user-events:<accountId>`. Milestones 2 and 3 are not needed.

## Global Constraints

- The whole local stack runs in Docker Compose (project name `chess-improver`). Every published port binds to `127.0.0.1`. New host ports: web `3100`, Mailpit UI `58025`, e2e fakes `53199`. Never stop, remove, prune or reconfigure containers, volumes or networks outside the `chess-improver` project. The host already runs `velvet_lab_db` on 5432. Always pass `-p chess-improver` or run from the repo root, and never run `docker system prune` or `docker volume prune`.
- ESM only. TypeScript `strict` and `noUncheckedIndexedAccess`. Workspace packages follow ADR 0026: a `development` condition comes first in `exports`. `apps/web` and `packages/ui` are the one recorded exception (ADR-WEB-1).
- Package scope `@ci/*`: new packages are `@ci/web` and `@ci/ui`.
- ADRs written in this milestone are named `docs/decisions/ADR-WEB-<n>-<slug>.md`. The controller renumbers them, and only Task 27 edits `docs/decisions/README.md`.
- Commits follow Conventional Commits and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Secrets come only from env, read and validated in `apps/web/src/env.ts`. Defaults are allowed only for non-secret dev values. `BETTER_AUTH_SECRET`, `GOOGLE_CLIENT_SECRET` and `CONTACT_EMAIL` have no default.
- Nothing is deployed, published or pushed to a registry, and no Vercel project is created (§18.5). Mail goes only to Mailpit.
- Chess libraries: react-chessboard (MIT) and chess.js (BSD-2) only. Never chessground, `@lichess-org/chessground` or chessops.
- Never use Aceternity UI, Magic UI, three/R3F, Lucide, GSAP or Lenis (GSAP and Lenis are landing-only, which is M5).
- Copy (§11 and frontend-design): sentence case, British spelling ("analysing", "colour"), active verbs, and an action keeps its name from button to toast ("Connect chess.com" leads to "chess.com connected"). Errors say what happened and how to fix it, with no apology. No `→` on links or buttons, no ALL-CAPS labels, no lorem ipsum. Notation uses the real minus sign U+2212.
- Quality floor: works at 360px with no horizontal page scroll, visible keyboard focus, WCAG AA contrast (text 4.5:1, large text and UI strokes 3:1), `prefers-reduced-motion` respected (no movement, marks appear in their final state), and a keyboard-operable board.
- Only tokens from `packages/ui/src/styles/tokens.css`. Raw hex values and arbitrary Tailwind values (`p-[13px]`) are banned outside that file, `packages/ui/src/board/**` and `packages/ui/src/annotations/**`. Enforced by `pnpm design:check` (Task 25).
- Third-party skills and CLIs (Impeccable, Vercel web-design-guidelines, gsap-skills) are read and reviewed, with the review recorded, before they are installed or run (Task 25). gsap-skills is deferred to M5.
- Only essential cookies (session, OAuth state, age block), so no cookie banner (§14). Cookie names start with `ci.` or `ci_`.
- Product pages (landing, report, area pages, explorers, onboarding analysis) belong to M5. This milestone builds only the shell, auth, linking, `/styleguide`, `/lab` and components.

## Review Focus

1. **Someone links a chess.com username that isn't theirs.** It must not block the real owner. Only the account holder who puts the code in the location field becomes the verified owner, and a later verification by the real owner takes over. Test: Task 19 `newest verified owner wins`, `unverified links do not block others`.
2. **The SSE connection drops while events keep arriving, including during the replay itself.** The reconnecting client must get every missed event exactly once and in order. If its cursor is older than the 7-day retention, it gets a `reset` and refetches. Test: Task 22 `replay plus live race delivers each id once in order`, `stale cursor emits reset`.
3. **Lichess sign-in fails on the provider side.** The user cancels consent (`error=access_denied`), or the Lichess email already belongs to a magic-link account. They must land on `/sign-in` with a plain message, not a raw error page. Test: Task 18 `consent denied returns to sign-in with message`, `email collision is not auto-linked`.
4. **A magic link is opened twice, or after it expires.** The user sees "This link has expired. Request a new one." and stays signed out. Test: Task 17 `reused magic link is rejected`.
5. **Long or dense real content at 360px:** 25-character usernames, long opening names, a 400-ply game, 12 timeline series. No page-level horizontal scroll, and charts scroll or compress inside their own container. Test: Task 13 `renders 400 plies`, Task 15 `timeline scrolls inside its container`, Task 28 `no horizontal scroll at 360px`.

---

## Lanes and dependencies

| Lane | Tasks | Owns (only this lane edits these) |
|---|---|---|
| **W0 Bootstrap and hot spots** | 1, 2, 3 | root `package.json`, `pnpm-lock.yaml`, `turbo.json`, `.gitignore`, `.env.example`, `docker-compose.yml`, `docker-compose.e2e.yml`, `docker/postgres/init.sql`, `apps/worker/docker-entrypoint.sh`, every `package.json` in `apps/web` and `packages/ui`, `packages/db/**`, `packages/ui/src/fixtures/**` |
| **W1 Auth, accounts, age gate, shell** | 17, 18, 19, 20, 21 | `apps/web/src/server/{auth,auth/**,accounts/**,chesscom/**,age.ts,mail.ts}`, `apps/web/src/lib/auth-client.ts`, `apps/web/src/app/(auth)/**`, `apps/web/src/app/(app)/**`, `apps/web/src/app/api/{auth,accounts,age}/**`, `apps/web/test/{auth,accounts,age,fakes,helpers/session.ts}`, `apps/web/e2e/{auth,accounts}.spec.ts` |
| **W2 Design system foundation** | 4, 5, 6, 7, 8, 9 | `packages/ui/DESIGN.md`, `packages/ui/design/**`, `packages/ui/src/{styles,motion,primitives,icons,overlays,lib}/**`, `apps/web/src/app/{layout.tsx,providers.tsx,globals.css}`, `apps/web/src/app/styleguide/{page.tsx,sections/foundations*.tsx,sections/primitives.tsx,sections/overlays.tsx}`, `apps/web/src/app/lab/styleguide-hero/**` |
| **W3 Board and annotations** | 10, 11, 12 | `packages/ui/src/{board,annotations}/**`, `apps/web/src/app/styleguide/sections/{board,annotations,eval-bar}.tsx`, `apps/web/src/app/lab/board/**` |
| **W4 Charts and data display** | 13, 14, 15, 16 | `packages/ui/src/{eval-graph,stats,area-score,timeline,progress}/**`, `apps/web/src/app/styleguide/sections/{eval-graph,stats,area-score,timeline,progress}.tsx` |
| **W5 Realtime** | 22, 23 | `apps/web/src/server/events/**`, `apps/web/src/app/api/events/**`, `apps/web/src/lib/{realtime/**,query-keys.ts}`, `apps/web/src/components/activity-indicator.tsx`, `apps/web/test/events/**` |
| **W6 Visual QA tooling** | 24, 25, 26, 28 | `scripts/**`, `.mcp.json`, `apps/web/playwright.config.ts`, `apps/web/e2e/{visual,quality,cookies}.spec.ts`, `apps/web/e2e/__screenshots__/**`, `apps/web/src/app/lab/{layout.tsx,page.tsx,lab-frame.tsx}`, `packages/ui/qa/**`, `.claude/skills/{design-system,chess-ui}/**` |
| **I Integration** | 27 | `CLAUDE.md`, `docs/decisions/README.md`, `.github/workflows/ci.yml`, final edits to `apps/web/src/app/(app)/layout.tsx` and `providers.tsx` (mounting W5 and W2 pieces) |

Each task writes its own ADR file. Every lane may add files under its own `test/` and `docs/decisions/ADR-WEB-*` paths. If a lane needs a new dependency after Task 1, it asks the W0 owner, who adds it in one commit.

```
M1 (incl. 12b) ──▶ T1 ──┬─▶ T2 ──┬─▶ T17 ──┬─▶ T18 ┐
                        │        │         ├─▶ T19 ├─▶ T21 (also needs T7, T8) ─┐
                        │        │         └─▶ T20 ┘                            │
                        │        └─▶ T22 (needs T17) ─▶ T23 ────────────────────┤
                        ├─▶ T3 ───────────────┐                                 │
                        ├─▶ T24, T25          │                                 │
                        └─▶ T4 ─▶ T5 ─┬─▶ T6 (needs T24) ─┐                     │
                                      ├─▶ T7 ─▶ T8 ───────┴─▶ T9                │
                                      ├─▶ T10 (needs T3, T24) ─▶ T11 ─▶ T12     │
                                      └─▶ T13 ∥ T14 ∥ T15 ∥ T16 (need T3)       │
   T9, T12, T13–T16 ─▶ T26        T21, T23, T26 ─▶ T27 ─▶ T28 ◀─────────────────┘
```

Maximum parallelism: after Task 1, W0 (T2, T3), W2 (T4), and W6 (T24, T25) run at once. After T5, W3, W4 (four tasks in parallel) and the rest of W2 run at once, while W1 and W5 proceed on their backend tasks.

## Shared procedures

**CL: critique loop.** This is used wherever a task says "Run CL on `<routes>`".
1. Run `pnpm shots --routes <routes>`. Expected: 4 PNGs per route in `.shots/<slug>/` (`390-light`, `390-dark`, `1440-light`, `1440-dark`).
2. Run `pnpm design:check`. Expected: `0 findings`. Any remaining finding is justified in the log.
3. Dispatch a **fresh** subagent (never the implementer of this task) with `packages/ui/qa/critique-prompt.md`, filled in with the PNG paths, `packages/ui/DESIGN.md` and `packages/ui/design/references/REFERENCES.md`. It gets no source code.
4. Fix every `blocker` and `major` issue. Append the pass to `packages/ui/qa/log/<surface>.md`: pass number, the ranked issues, the changes made, and what was tried and rejected.
5. Repeat. Stop at verdict `pass` after at least 2 passes. If the verdict is still not `pass` after the third pass, stop and escalate to the controller with the log.

**LP: lab procedure.** This is used for the styleguide hero and the board treatment.
1. Build three variants `a`, `b` and `c` on `/lab/<surface>/<variant>`, using real fixture data (Task 3). Each variant must differ on the named axes in its task, not only in colour.
2. Run `pnpm shots --routes /lab/<surface>/a,/lab/<surface>/b,/lab/<surface>/c`.
3. A fresh subagent ranks the three against DESIGN.md and the ban list, with reasons.
4. The implementer picks one and writes `packages/ui/design/decisions/<surface>.md`: the choice, why, what was taken from the losers, and the screenshot paths. The variant pages stay; they are dev-only.

---

## W0: Bootstrap and hot spots

### Task 1: Workspace scaffold, dependencies and Compose services

**Files:**
- Create: `apps/web/{package.json,tsconfig.json,next.config.ts,Dockerfile,vitest.config.ts,src/env.ts,src/brand.ts,src/app/layout.tsx,src/app/page.tsx,src/app/api/health/route.ts}`, `apps/web/test/env.test.ts`, `apps/web/test/setup.ts`
- Create: `packages/ui/{package.json,tsconfig.json,vitest.config.ts,src/lib/cn.ts,src/lib/index.ts}`, `packages/ui/test/cn.test.ts`
- Create: `docker-compose.e2e.yml`, `docs/decisions/ADR-WEB-1-source-only-ui-packages.md`, `docs/decisions/ADR-WEB-2-web-dev-stack.md`
- Modify: root `package.json`, `pnpm-lock.yaml`, `turbo.json` (`globalPassThroughEnv`), `.gitignore` (`.shots/`, `apps/web/test-results/`, `apps/web/playwright-report/`), `.env.example`, `docker-compose.yml`, `docker/postgres/init.sql` (`CREATE DATABASE chess_web_test;`), `apps/worker/docker-entrypoint.sh`

**Interfaces:**
- Produces:
  - `getEnv(): WebEnv` (zod-parsed, cached). Fields: `DATABASE_URL`, `REDIS_URL`, `BETTER_AUTH_SECRET`, `BETTER_AUTH_URL` (default `http://localhost:3100`), `BETTER_AUTH_TRUSTED_ORIGINS` (comma list, default `""`), `SMTP_URL` (default `smtp://mailpit:1025`), `MAIL_FROM` (default `chess-improver <no-reply@chess-improver.local>`), `GOOGLE_CLIENT_ID?`, `GOOGLE_CLIENT_SECRET?`, `LICHESS_CLIENT_ID` (default `chess-improver`), `LICHESS_AUTHORIZE_BASE_URL` (default `https://lichess.org`), `LICHESS_API_BASE_URL` (default `https://lichess.org`), `CHESSCOM_API_BASE_URL` (default `https://api.chess.com`), `CHESSCOM_ENABLED` (`"1"` → true, default false), `CHESSCOM_OAUTH_ENABLED` (default false), `CONTACT_EMAIL`, `LAB_ENABLED` (default false).
  - `brand = { name: "chess-improver", wordmark: "chess-improver" } as const` in `apps/web/src/brand.ts`. This is the one file to change for the product name (§18.4).
  - `cn(...inputs: ClassValue[]): string` (clsx + tailwind-merge) from `@ci/ui/lib`.
  - `@ci/ui` exports map: `"./*": { "development": "./src/*/index.ts", "default": "./src/*/index.ts" }` plus `"./tokens.css": "./src/styles/tokens.css"`. Each component folder has its own `index.ts`, so no lane shares a barrel.
  - `GET /api/health` → `200 { ok: true, db: true, redis: true }`, or `503` with the failing flag false.
  - Root scripts `shots` (`tsx scripts/shots.ts`), `design:check` (`tsx scripts/ban-lint.ts && pnpm dlx impeccable@<reviewed> detect apps/web/src packages/ui/src`; the version is filled in by Task 25), `e2e` (`pnpm --filter @ci/web e2e`).
  - Compose services:
    - `deps`: one-shot `pnpm install --frozen-lockfile`.
    - `web`: build `apps/web/Dockerfile` target `dev`, `next dev --port 3100 --hostname 0.0.0.0`, port `127.0.0.1:3100:3100`, depends on `deps` (completed), `postgres` and `redis` (healthy), plus `mailpit`.
    - `mailpit`: `axllent/mailpit` pinned to the current tag, UI `127.0.0.1:58025:8025`, SMTP 1025 internal only.
    - `fakes`: profile `e2e`, runs `apps/web/test/fakes/main.ts`, port `127.0.0.1:53199:3199`. Task 18 writes that file.
    - `qa`: profile `qa`, `mcr.microsoft.com/playwright:v1.65.1-noble`, repo mounted, used for visual baselines.
  - `docker-compose.e2e.yml` override for `web`: `LICHESS_AUTHORIZE_BASE_URL=http://localhost:53199`, `LICHESS_API_BASE_URL=http://fakes:3199`, `CHESSCOM_API_BASE_URL=http://fakes:3199/chesscom`, `CHESSCOM_ENABLED=1`.

- [ ] **Step 1:** Run `npm view <pkg> version license` for every package in Tech Stack plus `@tanstack/react-query`, `nodemailer`, `@testing-library/react`, `jsdom`, `@axe-core/playwright`, `tailwind-merge`, `clsx`, `class-variance-authority` and `tw-animate-css`. Record them in ADR-WEB-1. Any licence that is not MIT, ISC, BSD or Apache stops the task and goes to the controller.
- [ ] **Step 2:** Check the host ports are free: `netstat -ano | findstr ":3100 :58025 :53199"`. Expected: no LISTENING lines. If a port is taken, pick the next free one and record it in ADR-WEB-2.
- [ ] **Step 3: Write failing tests.** `apps/web/test/env.test.ts`:
  - `getEnv` throws, naming `BETTER_AUTH_SECRET`, when it is missing.
  - `CHESSCOM_ENABLED: "1"` parses to `true`, and absent parses to `false`.
  - `LICHESS_CLIENT_ID` defaults to `"chess-improver"`.

  `packages/ui/test/cn.test.ts`: `cn("p-2", "p-4")` equals `"p-4"`.
- [ ] **Step 4:** Run `pnpm --filter @ci/web --filter @ci/ui test`. Expected: FAIL (modules missing).
- [ ] **Step 5: Implement the scaffold.**
  - Install every runtime and dev dependency for this milestone in one commit, so later lanes never touch the lockfile.
  - `apps/web/tsconfig.json` extends the base with `module: "ESNext"`, `moduleResolution: "Bundler"`, `jsx: "react-jsx"`, `lib: ["ES2023", "DOM", "DOM.Iterable"]`, and the Next plugin. Relative imports in `apps/web` and `packages/ui` are extensionless (ADR-WEB-1).
  - `next.config.ts`: `transpilePackages: ["@ci/ui", "@ci/db", "@ci/chess-core"]`. Make Next resolve the `development` condition for `@ci/db` and `@ci/chess-core`. Check the current Turbopack and webpack option with context7. If none exists, alias both to their `src/index.ts` in development only.
  - The worker entrypoint skips install when `SKIP_INSTALL=1`. Compose sets it on `worker` and `web`, and both depend on `deps`, so two installs never race on the shared volume.
  - Add `web_node_modules` and `ui_node_modules` volumes.
- [ ] **Step 6:** Run the tests. Expected: PASS.
- [ ] **Step 7: Verify the stack.** On an existing volume, first run `docker compose exec postgres psql -U postgres -c "CREATE DATABASE chess_web_test"` (ADR 0023 addendum). Then run `docker compose up -d --build web` and `curl -s localhost:3100/api/health`. Expected: `{"ok":true,"db":true,"redis":true}`.
- [ ] **Step 8: Verify hot reload.** Edit the text in `src/app/page.tsx` on the host, then run `curl -s localhost:3100/`. Expected: the new text within 5 s. If it doesn't appear, enable polling (`WATCHPACK_POLLING=true`, or the `watchOptions.pollIntervalMs` Next config) and record which one worked in ADR-WEB-2.
- [ ] **Step 9:** Run `pnpm lint && pnpm typecheck && pnpm test`. Expected: all pass, and the worker tests are unchanged.
- [ ] **Step 10:** Commit `chore(web): scaffold next app, ui package and compose services`.

### Task 2: Auth, linking and age schema

**Files:**
- Create: `packages/db/src/auth-schema.ts`, `packages/db/src/web-schema.ts`, `packages/db/src/events.ts`, `packages/db/src/migrate.ts`, generated `packages/db/drizzle/00NN_web_auth.sql`, tests `packages/db/test/web-schema.test.ts`, `packages/db/test/events.test.ts`
- Create: `docs/decisions/ADR-WEB-3-auth-and-link-data-model.md`
- Modify: `packages/db/src/index.ts` (export the new files), `packages/db/test/setup.ts` (truncate the new tables)

**Interfaces:**
- Produces:
  - Better Auth core tables with **uuid** ids: `user` (`id`, `name`, `email` unique, `emailVerified`, `image`, `createdAt`, `updatedAt`, plus `birthMonth text null` (`YYYY-MM`), `ageCheckedAt timestamptz null`, `marketingEmail boolean not null default false`, `publicShareDefault boolean not null default false`), `session`, `account` (with `accessTokenExpiresAt`), `verification`. Column names follow the Better Auth core schema for 1.7.
  - `accountLinks`: `id uuid pk`, `userId` (fk `user`, cascade), `linkedAccountId` (fk `linked_accounts`), `method` (enum `link_method`: `username`, `location_code`, `lichess_oauth`, `chesscom_oauth`), `verifiedAt timestamptz null`, `createdAt`. Constraints: unique `(userId, linkedAccountId)`, and a **partial unique index on `linkedAccountId` where `verifiedAt is not null`** (one verified owner).
  - `chesscomVerifications`: `id uuid pk`, `accountLinkId` (fk, cascade, unique), `code text`, `expiresAt`, `lastCheckedAt null`, `checks int default 0`, `createdAt`.
  - `userEventsChannel(accountId: string): string` (returns `user-events:${accountId}`, the same naming as the worker).
  - `type StreamEvent = { id: number; accountId: string; type: string; payload: unknown }`.
  - `listEventsAfter(db: Db, accountIds: string[], afterId: number, limit?: number): Promise<StreamEvent[]>` (ascending id, default limit 500).
  - `oldestEventId(db: Db, accountIds: string[]): Promise<number | null>`.
  - `runMigrations(db: Db): Promise<void>`.
  - `linked_accounts.userId` is left in place for the M1 CLI and marked `@deprecated` in a comment. The web app uses only `accountLinks`.

- [ ] **Step 1: Write failing tests.**
  - `web-schema.test.ts`:
    - Two users can each hold an unverified link to the same `linked_accounts` row.
    - Setting `verifiedAt` on both throws a unique violation.
    - Deleting a user cascades its `session`, `account` and `accountLinks` rows, but not `linked_accounts` or `games`.
    - `marketingEmail` and `publicShareDefault` default to `false`.
  - `events.test.ts`:
    - Insert 3 events across 2 accounts.
    - `listEventsAfter(db, [a1], 0)` returns only `a1`'s events in ascending id order.
    - `listEventsAfter(db, [a1, a2], firstId)` excludes `firstId`.
    - `oldestEventId` returns the smallest id.
    - `userEventsChannel("x")` equals `"user-events:x"`.
- [ ] **Step 2:** Run `docker compose run --rm worker pnpm --filter @ci/db test`. Expected: FAIL.
- [ ] **Step 3: Implement the tables** and generate the migration with `pnpm --filter @ci/db generate`. Write ADR-WEB-3 covering: the join table instead of `linked_accounts.userId`, the single-verified-owner rule, and uuid auth ids, because `linked_accounts.user_id` is a uuid. After this task, only Task 17 may regenerate `auth-schema.ts` (with the Better Auth CLI), and only W1 generates further web migrations.
- [ ] **Step 4:** Run the tests. Expected: PASS, and the existing db tests still pass.
- [ ] **Step 5:** Commit `feat(db): auth tables, account links, chess.com verifications and event queries`.

### Task 3: Real game fixtures

**Files:**
- Create: `scripts/export-fixtures.ts` (W6 owns `scripts/`, but this one file is written here and its ownership noted in its header), `packages/ui/src/fixtures/{index.ts,types.ts,schema.ts}`, `packages/ui/src/fixtures/games/{middlegame-blunder,endgame-grind,short-attack}.json`, `packages/ui/src/fixtures/{kpis,area-scores,timeline}.ts`, `packages/ui/test/fixtures.test.ts`

**Interfaces:**
- Produces, from `@ci/ui/fixtures`:
  - `interface FixturePly { ply: number; color: "white" | "black"; san: string; uci: string; fenAfter: string; winWhite: number; cpWhite: number | null; mateWhite: number | null; judgement: "inaccuracy" | "mistake" | "blunder" | null; bestUci: string | null; clockSeconds: number | null; phase: "opening" | "middlegame" | "endgame" }`. Evals are converted to **White's** point of view on export.
  - `interface GameFixture { id: string; source: string /* game URL */; white: { name: string; rating: number | null }; black: { name: string; rating: number | null }; playerColor: "white" | "black"; result: "1-0" | "0-1" | "1/2-1/2"; timeClass: string; eco: string | null; openingName: string | null; startFen: string; plies: FixturePly[] }`.
  - `games: { middlegameBlunder: GameFixture; endgameGrind: GameFixture; shortAttack: GameFixture }`.
  - `kpiFixtures`, `areaScoreFixtures` and `timelineFixtures`, typed by the props in Tasks 14 and 15 and marked `synthetic: true`. These are shaped like §8b and §12b, but the numbers are made up until M2 lands.

- [ ] **Step 1:** Write the failing test `fixtures.test.ts`:
  - Every game fixture parses with the zod schema.
  - `plies[i].ply === i + 1`.
  - Replaying the `uci` moves with chess.js from `startFen` reproduces each `fenAfter`.
  - `middlegameBlunder` has at least 1 `blunder`.
  - `endgameGrind` has more than 100 plies and reaches the `endgame` phase.
  - `areaScoreFixtures` lists exactly the seven areas in this order: openings, middlegame, endgame, tactics, calculation, time, mindset.
  - `timelineFixtures` uses all six states.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3:** Run the M1 CLI on public games: `docker compose run --rm worker pnpm --filter @ci/worker cli analyse --platform lichess --username DrNykterstein --max 20`. Then run `tsx scripts/export-fixtures.ts --game <id> --name <name>` to choose three games that match the three shapes and write the JSON files.
- [ ] **Step 4:** Run the test. Expected: PASS.
- [ ] **Step 5:** Commit `feat(ui): real analysed game fixtures and synthetic kpi fixtures`.

---

## W2: Design system foundation

### Task 4: DESIGN.md, ban list and annotated references

**Files:**
- Create: `packages/ui/DESIGN.md`, `packages/ui/design/references/REFERENCES.md`, plus 4 reference PNGs in `packages/ui/design/references/`

**Interfaces:**
- Produces: the named ban list IDs `B1`–`B18`, the token names in the table below, and the motion principles. Every later task cites these.

- [ ] **Step 1: Run the frontend-design plan pass.** Load `frontend-design:frontend-design`. Write the subject, audience and job: "a 1000–2000 club player reviewing their own games on a laptop or phone; the job is to see, precisely, where they went wrong and what to practise". Then write the direction (Annotated scoresheet, §11), the 4–6 named colours, the type roles, an ASCII layout concept for the styleguide hero, and the principles. Spend boldness only on the board and the eval graph, and keep everything else quiet. Then review the plan against the five cliché looks and record what changed. One note is needed: ruled lines are scoresheet-native but border on cliché 3 (broadsheet). Ruled lines appear only on scoresheet and move-list surfaces. Panels keep `--radius-sheet`, and there are no newspaper columns.
- [ ] **Step 2: Write DESIGN.md** with these sections, using exactly these values:

  **Colour tokens** (light "day" / dark "night study"). Contrast is checked against `sheet` and `paper`.

  | Token | Day | Night | Role |
  |---|---|---|---|
  | `--color-paper` | `#F3F5F8` | `#121A2B` | page background |
  | `--color-sheet` | `#FBFCFD` | `#18223A` | raised surfaces, scoresheet, panels |
  | `--color-well` | `#E7EBF1` | `#0D1421` | inset wells, inputs on sheet |
  | `--color-rule` | `#C9D1DD` | `#2C3956` | decorative lines (non-text) |
  | `--color-rule-strong` | `#7A879E` | `#6B7B9B` | input borders, chart axes (≥ 3:1) |
  | `--color-ink` | `#16233B` | `#E7ECF4` | text, primary fill, focus ring |
  | `--color-ink-soft` | `#3D4A63` | `#B4BFD2` | secondary text |
  | `--color-ink-faint` | `#5B6780` | `#8E9AB1` | tertiary text (lowest allowed) |
  | `--color-on-ink` | `#FBFCFD` | `#121A2B` | text on ink fills |
  | `--color-pen` | `#D3312B` | `#F2695E` | red-pen strokes and marks (spec value) |
  | `--color-pen-text` | `#C42B26` | `#F2695E` | red text (AA on `well` too) |
  | `--color-good` | `#2F7D5B` | `#5DBB8E` | good-move strokes and marks |
  | `--color-good-text` | `#2A7353` | `#5DBB8E` | green text |
  | `--color-pencil` | `#56657F` | `#97A6C0` | neutral annotations, plan arrows |
  | `--color-board-light` | `#E3E8EF` | `#7F8DA8` | light squares |
  | `--color-board-dark` | `#A7B4C7` | `#55637F` | dark squares |
  | `--color-board-lastmove` | `rgb(22 35 59 / 0.16)` | `rgb(231 236 244 / 0.18)` | last-move tint |

  The halo rule: every board stroke has a 2px `--color-sheet` halo underneath it, so a mark keeps ≥ 3:1 on either square colour.

  **Type.** Literata (variable, `opsz` axis) handles headlines, annotations and notation. Schibsted Grotesk handles UI. Both load through `next/font/google`. Weights: Literata 400 and 600, with italic 400 only for whole annotation comments. Schibsted 400, 500 and 700. Scale (size/line-height in px, from the classic scale): `xs 12/16`, `sm 14/20`, `base 16/24`, `prose 18/30` (serif body, which gets more leading), `lg 18/26`, `xl 21/28`, `2xl 24/30`, `3xl 36/42`, `4xl 48/54`, `5xl 60/64`. Notation uses `font-variant-numeric: lining-nums tabular-nums` with Literata 600 glyphs in pen colour. Measure: ≤ 72ch for serif prose and ≤ 66ch for sans.

  **Radii by role:** `--radius-board 2px`, `--radius-field 6px`, `--radius-sheet 10px`, `--radius-pill 9999px`. **Elevation:** there are no card shadows; surfaces separate by tone (`paper` → `sheet` → `well`). The one shadow is `--shadow-lift: 0 10px 30px -12px rgb(22 35 59 / 0.35)` (night: `rgb(0 0 0 / 0.5)`), and it is used only for popovers, the drawer and the command menu.

  **Motion:** `--dur-instant 90ms`, `--dur-quick 160ms`, `--dur-base 240ms`, `--dur-draw 420ms`, `--dur-reveal 600ms`. `--ease-out cubic-bezier(0.2, 0.7, 0.2, 1)` and `--ease-pen cubic-bezier(0.6, 0.05, 0.3, 1)`. Springs: `piece { stiffness: 520, damping: 42 }` and `evalBar { stiffness: 260, damping: 30 }`. Annotation stagger is 120ms.

  The principles:
  1. Only user-caused or orchestrated moments animate.
  2. Pen strokes draw by `pathLength`. A circle starts at about 1 o'clock and overruns to 380°, like a hand.
  3. Arrowheads appear after 85% of the shaft.
  4. Nothing fades up on scroll.
  5. There are no hover lifts.
  6. With reduced motion, everything is shown in its final state at once.

  **Ban list**:
  - `B1` warm cream with terracotta.
  - `B2` near-black with one acid accent.
  - `B3` broadsheet: hairlines everywhere, zero radius, newspaper columns.
  - `B4` SaaS card kit: identical rounded cards, one radius, grey shadow.
  - `B5` template chrome: ALL-CAPS tracked eyebrows, `A · B · C` meta strings, `WORD — fragment` labels, `#0B0B0B`/`#111`, monospace data labels, `→` on links.
  - `B6` default fonts: Inter, Geist, Space Grotesk, Instrument Serif, the system stack.
  - `B7` purple or indigo accents, gradient text or washes, glows, glassmorphism.
  - `B8` a badge above an H1.
  - `B9` icon-topped 3-up feature grids, stat-banner rows, "1-2-3 steps" unless the content is a sequence.
  - `B10` coloured top or left borders on cards.
  - `B11` fade-up on every section, hover-lift on every card, parallax, scroll-jacking.
  - `B12` Aceternity, Magic UI, three.js, chessground, chessops.
  - `B13` Lucide or emoji icons.
  - `B14` stock shadcn tokens (zinc, `--radius: 0.625rem`, Vega style).
  - `B15` lorem ipsum or invented chess data.
  - `B16` accenting one word in a headline (italic, bold or colour).
  - `B17` grey body text on dark.
  - `B18` blocking spinners. Use the progress placeholder instead.
- [ ] **Step 3: Capture 4 references** with Playwright into `packages/ui/design/references/`. They are internal reference only and never shipped.
  1. A lichess.org game analysis page. Take its move-list density and its eval graph under the board. Don't take its board colours or chrome.
  2. A Lichess study chapter with drawn arrows and circles. Take its arrow semantics. Don't take its uniform, mechanical strokes.
  3. A public-domain scan of a handwritten scoresheet from Wikimedia Commons. Take the ruled grid, the ink weight and the red correction marks.
  4. A public-domain page from an annotated 19th-century chess book with a diagram (Wikimedia Commons). Take the notation typography and the diagram captioning.

  `REFERENCES.md` says, for each one: what to take, what not to copy, and the source URL and licence.
- [ ] **Step 4: Review.** A fresh subagent reads DESIGN.md only. It answers: does any section read like a default for any similar product, and does any token pair fail its stated contrast? Fix what it flags and record the changes at the end of DESIGN.md under "Revisions".
- [ ] **Step 5:** Commit `docs(ui): design direction, tokens, ban list and references`.

### Task 5: Tokens, fonts, themes and the styleguide skeleton

**Files:**
- Create: `packages/ui/src/styles/tokens.css`, `packages/ui/src/motion/{index.ts,tokens.ts,use-reduced-motion.ts}`, `packages/ui/test/tokens.test.ts`, `packages/ui/test/motion.test.ts`, `apps/web/src/app/globals.css`, `apps/web/src/app/providers.tsx`, `apps/web/src/app/styleguide/page.tsx`, `apps/web/src/app/styleguide/stub-section.tsx`, and stub files `apps/web/src/app/styleguide/sections/{foundations,primitives,overlays,board,annotations,eval-bar,eval-graph,stats,area-score,timeline,progress}.tsx`, `docs/decisions/ADR-WEB-7-design-tokens.md`
- Modify: `apps/web/src/app/layout.tsx`

**Interfaces:**
- Produces:
  - Tailwind `@theme` variables with exactly the Task 4 names. Night values sit under `[data-theme="dark"]`, and dark mode is declared as `@custom-variant dark (&:where([data-theme=dark], [data-theme=dark] *))`. `--font-serif` and `--font-sans` come from the next/font variables.
  - `motion` tokens in JS: `dur = { instant: 0.09, quick: 0.16, base: 0.24, draw: 0.42, reveal: 0.6 }` (seconds), `ease = { out: [0.2, 0.7, 0.2, 1], pen: [0.6, 0.05, 0.3, 1] }`, `spring = { piece: { type: "spring", stiffness: 520, damping: 42 }, evalBar: { type: "spring", stiffness: 260, damping: 30 } }`, `stagger = 0.12`.
  - `useReducedMotion(): boolean`.
  - `Providers` (next-themes `attribute="data-theme"`, `defaultTheme="system"`, `enableColorScheme`). Task 27 adds the query and realtime providers.
  - The `/styleguide` page imports one named export per section file (`FoundationsSection`, `PrimitivesSection`, `OverlaysSection`, `BoardSection`, `AnnotationsSection`, `EvalBarSection`, `EvalGraphSection`, `StatsSection`, `AreaScoreSection`, `TimelineSection`, `ProgressSection`). Each stub renders `<StubSection id="<kebab>" title="<Title>" />`, and every section root carries `data-section="<kebab>"`. Each owning lane replaces only its own file. The page is guarded like `/lab`: when `NODE_ENV === "production"` and `LAB_ENABLED !== "1"`, it returns `notFound()`.

- [ ] **Step 1: Write failing tests.**
  - `tokens.test.ts` parses `tokens.css` and asserts, for both themes, WCAG ratios: `ink`, `ink-soft`, `ink-faint`, `pen-text` and `good-text` are ≥ 4.5 against `paper`, `sheet` and `well`. `pen`, `good`, `pencil` and `rule-strong` are ≥ 3 against `sheet`. `pen` is ≥ 3 against the halo colour (`sheet`). `on-ink` is ≥ 4.5 against `ink`. It also asserts that no token value is `#0B0B0B` or `#111111`, and that both themes define the same set of names.
  - `motion.test.ts`: `dur.draw === 0.42`, and `useReducedMotion()` returns true when `matchMedia("(prefers-reduced-motion: reduce)")` matches.
- [ ] **Step 2:** Run `pnpm --filter @ci/ui test`. Expected: FAIL.
- [ ] **Step 3: Implement.** Write `tokens.css`. Wire the fonts in `layout.tsx` (`Literata` with `axes: ["opsz"]`, plus `Schibsted_Grotesk`) and the `Providers`. Build the styleguide skeleton. Write ADR-WEB-7, which splits `pen` and `pen-text` (the spec's `#D3312B` is 4.14:1 on `well`, below AA for text) and sets the halo rule.
- [ ] **Step 4:** Run the tests. Expected: PASS. Then run `curl -s localhost:3100/styleguide`. Expected: 200, and all 11 `data-section` ids are present.
- [ ] **Step 5:** Commit `feat(ui): annotated-scoresheet tokens, fonts, themes and styleguide skeleton`.

### Task 6: Styleguide hero lab

**Files:**
- Create: `apps/web/src/app/lab/styleguide-hero/{a,b,c}/page.tsx`, `packages/ui/design/decisions/styleguide-hero.md`

**Interfaces:**
- Consumes: `LabFrame` and the lab guard (Task 24); `games.middlegameBlunder` (Task 3); tokens (Task 5). The board is a static image placeholder made from token-coloured SVG squares, because Task 11 isn't ready yet. The hero is judged on type, layout and annotation language, not on the board widget.

- [ ] **Step 1:** Run LP with three variants. All three use the same critical moment from `middlegameBlunder`: notation like `31. Rxe6?? −4.2` plus a one-sentence coach note.
  - **a "Scoresheet margin":** a ruled two-column scoresheet, with the board sitting in the margin and the red-pen mark on the move row.
  - **b "Book plate":** the board as a captioned figure, with the notation set large underneath in Literata and the note as a footnote.
  - **c "Coach's note":** the notation as the headline type element at `5xl`, with the board offset beside it and a handwritten-style pen circle linking them.

  The axes that must differ: where the notation sits in the hierarchy, how the board and text relate, and how the red-pen mark connects them.
- [ ] **Step 2:** Write the decision file (LP step 4).
- [ ] **Step 3:** Commit `feat(web): styleguide hero lab variants and decision`.

### Task 7: Primitives from shadcn (Base UI) with rewritten tokens, and Phosphor icons

**Files:**
- Create: `packages/ui/components.json`, `packages/ui/src/primitives/{index.ts,button.tsx,input.tsx,field.tsx,dialog.tsx,popover.tsx,tabs.tsx,switch.tsx,tooltip.tsx,kbd.tsx}`, `packages/ui/src/icons/{index.ts,icon.tsx}`, `packages/ui/test/primitives.test.tsx`

**Interfaces:**
- Produces:
  - `Button` with props `{ variant?: "primary" | "outline" | "quiet" | "danger"; size?: "sm" | "md"; asChild?: boolean } & ButtonHTMLAttributes`. Fills: primary = ink fill with `on-ink` text, outline = `rule-strong` border, quiet = text only, danger = `pen-text` border and text. Radius `--radius-field`. Focus is `outline: 2px solid var(--color-ink); outline-offset: 2px` on `:focus-visible`.
  - `Input`, and `Field` `{ label: string; hint?: string; error?: string; children }`. Field wires `aria-describedby` and `aria-invalid`.
  - `Dialog`, `Popover`, `Tabs`, `Switch` and `Tooltip`, each a Base UI primitive restyled with the tokens.
  - `Kbd`.
  - `Icon` with props `{ icon: PhosphorIcon; label?: string; size?: 16 | 20 | 24; weight?: "regular" | "bold" | "fill" }`. It is `aria-hidden` unless `label` is set, and defaults to `size 20`, `weight "regular"`.

- [ ] **Step 1:** Write failing tests:
  - `Button` renders `variant="primary"` with the ink classes.
  - `Field` with `error="Enter a valid email address."` sets `aria-invalid="true"`, and the input's `aria-describedby` points at the error text.
  - `Icon` without a label has `aria-hidden="true"`, and with `label="Settings"` it has `role="img"` and `aria-label="Settings"`.
  - `Switch` toggles on Space.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3: Implement.** Run `pnpm dlx shadcn@4.23.0 init` in `packages/ui` with base **Base UI** and style **Mira** (never Vega, which is B14). Add the components, then rewrite every class and token to the Task 4 names. Delete any zinc or neutral variables and any `lucide-react` import the CLI added.
- [ ] **Step 4:** Run the tests, then `pnpm design:check`. Expected: PASS and `0 findings`.
- [ ] **Step 5:** Commit `feat(ui): base ui primitives restyled to scoresheet tokens, phosphor icon wrapper`.

### Task 8: Toast, drawer and command menu

**Files:**
- Create: `packages/ui/src/overlays/{index.ts,toaster.tsx,drawer.tsx,command-menu.tsx}`, `packages/ui/test/overlays.test.tsx`

**Interfaces:**
- Produces:
  - `Toaster` (sonner, themed, `position` bottom-center below 640px and bottom-right above it) and `toast` re-exported from sonner.
  - `Drawer` (vaul), with parts `Drawer.Root`, `.Trigger`, `.Content`, `.Title` and `.Description`. It uses `--shadow-lift` and `--radius-sheet` on the top corners only.
  - `CommandMenu` with props `{ items: { id: string; label: string; group: string; keywords?: string[]; onSelect: () => void }[]; open?: boolean; onOpenChange?: (open: boolean) => void }`. It opens on Ctrl+K and Cmd+K, and its placeholder is `"Search pages and actions"`.

- [ ] **Step 1:** Write failing tests:
  - Ctrl+K opens the command menu.
  - Typing `acc` filters to the item labelled `Accounts`.
  - Enter calls its `onSelect` and closes the menu.
  - Escape closes it.
  - `toast("chess.com connected")` renders that text inside `Toaster`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(ui): toaster, drawer and command menu`.

### Task 9: Styleguide foundations, primitives and overlays, plus critique

**Files:**
- Modify: `apps/web/src/app/styleguide/page.tsx` (adds the chosen hero from Task 6), `sections/foundations.tsx` (colour, type scale, radii, elevation and motion tokens, rendered from the CSS variables, not retyped), `sections/primitives.tsx`, `sections/overlays.tsx`
- Create: `packages/ui/qa/log/styleguide-foundations.md`

- [ ] **Step 1:** Build the three sections and the hero, using real copy from fixtures, not placeholder text. Every primitive is shown in its default, hover, focus-visible, disabled and error states.
- [ ] **Step 2:** Run CL on `/styleguide` (foundations, primitives and overlays only).
- [ ] **Step 3:** Commit `feat(web): styleguide foundations with critique passes`.

---

## W3: Board and annotations

### Task 10: Board treatment lab

**Files:**
- Create: `apps/web/src/app/lab/board/{a,b,c}/page.tsx`, `packages/ui/design/decisions/board.md`

**Interfaces:**
- Consumes: react-chessboard 5.12.1 directly (the wrapper comes in Task 11); `games.middlegameBlunder` at its blunder ply; tokens.

- [ ] **Step 1:** Check the licence of the react-chessboard default piece set (`npm pack react-chessboard@5.12.1` into a scratch directory and read its piece sources). Record it in `board.md`. If the licence isn't MIT, BSD or CC0-compatible for a closed-source site, variant `b` or `c` must use a piece set with a clear permissive licence, and the choice goes to the controller.
- [ ] **Step 2:** Run LP with three variants, each showing the same position with the same 1 arrow, 1 circle and 1 `??` mark.
  - **a "Printed diagram":** flat day squares, a 1px `rule-strong` frame, coordinates outside the board in Schibsted `xs`, and a fine 0.08-square pen stroke.
  - **b "Book hatching":** dark squares drawn as diagonal ink hatching at low opacity over `board-light`, like a 19th-century diagram, with a 0.11-square marker stroke.
  - **c "Ink well":** the board set inside a `well` with `--radius-board`, coordinates on the squares, and a pen stroke with tapered ends.

  The axes that must differ: the square treatment, where the coordinates go, and the stroke character.
- [ ] **Step 3:** Write the decision file, including the stroke width, the arrowhead geometry and the piece set chosen.
- [ ] **Step 4:** Commit `feat(web): board treatment lab and decision`.

### Task 11: Board, notation and eval bar

**Files:**
- Create: `packages/ui/src/board/{index.ts,board.tsx,square-grid.tsx,notation.ts,notation-text.tsx,eval-bar.tsx,theme.ts}`, `packages/ui/test/board.test.tsx`, `packages/ui/test/notation.test.ts`, `packages/ui/test/eval-bar.test.tsx`
- Modify: `apps/web/src/app/styleguide/sections/{board,eval-bar}.tsx`

**Interfaces:**
- Consumes: `winPercent` from `@ci/chess-core`; `spring.piece` and `spring.evalBar`; the Task 10 decision.
- Produces:
  - `type Square = \`${"a"|"b"|"c"|"d"|"e"|"f"|"g"|"h"}${1|2|3|4|5|6|7|8}\``.
  - `Board` with props `{ fen: string; label: string; orientation?: "white" | "black"; lastMove?: { from: Square; to: Square }; check?: Square; annotations?: Annotation[]; animateAnnotations?: boolean; interactive?: boolean; onMove?: (m: { from: Square; to: Square; promotion?: "q" | "r" | "b" | "n" }) => boolean }`. `Annotation` comes from Task 12, imported as a type only. Until Task 12 lands, Board renders no overlay when `annotations` is set.
  - Keyboard access through `SquareGrid`: an 8×8 grid of transparent `button`s over the board with `pointer-events: none`. It has a roving tabindex. The arrow keys move focus, Enter or Space selects and then moves, and Escape cancels. Each square's `aria-label` reads like `"e4, white knight"` or `"e5, empty"`. Moves are checked with chess.js.
  - `formatEval(score: { cp: number } | { mate: number }): string` and `formatNotation(m: { moveNumber: number; color: "white" | "black"; san: string; judgement?: "inaccuracy" | "mistake" | "blunder" | null; score?: { cp: number } | { mate: number } }): string`.
  - `NotationText` renders the result, with the judgement glyph in `pen-text`.
  - `EvalBar` with props `{ score: { cp: number } | { mate: number }; orientation?: "white" | "black"; label: string }`. Its fill height is `winPercent` for White, and it moves with `spring.evalBar`.

- [ ] **Step 1: Write failing tests.**

```ts
expect(formatEval({ cp: -420 })).toBe("−4.2");
expect(formatEval({ cp: 35 })).toBe("+0.4");   // Math.round(|cp| / 10) / 10, so the float rounding is pinned
expect(formatEval({ cp: 4 })).toBe("0.0");
expect(formatEval({ mate: 3 })).toBe("#3");
expect(formatEval({ mate: -2 })).toBe("#−2");
expect(formatNotation({ moveNumber: 31, color: "white", san: "Rxe6", judgement: "blunder", score: { cp: -420 } })).toBe("31. Rxe6?? −4.2");
expect(formatNotation({ moveNumber: 31, color: "black", san: "Kg7" })).toBe("31… Kg7");
expect(formatNotation({ moveNumber: 12, color: "white", san: "Nf5", judgement: "inaccuracy" })).toBe("12. Nf5?!");
```
  - `board.test.tsx`:
    - From the start position, Tab focuses the board grid. Pressing ArrowUp from `e1` focuses `e2` (white orientation).
    - Pressing Enter on `e2`, ArrowUp twice and Enter calls `onMove({ from: "e2", to: "e4" })`.
    - An illegal keyboard move doesn't call `onMove`.
    - With `orientation="black"`, ArrowUp from `e8` focuses `e7`.
    - The square `e1` has `aria-label="e1, white king"`.
  - `eval-bar.test.tsx`: `{ cp: 0 }` gives a fill of 50%, and with reduced motion the fill is set at once (no spring).
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement on react-chessboard v5 using the square and piece styling from `theme.ts`, built from the tokens. Check the v5 options API with context7. The last move is drawn as the `board-lastmove` tint on both squares, and the check square gets a `pen` ring (no radial gradient, B7).
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Fill the `board` and `eval-bar` styleguide sections with fixture positions in both orientations, then run CL on `/styleguide` (board and eval bar).
- [ ] **Step 6:** Commit `feat(ui): board with keyboard access, notation formatting and eval bar`.

### Task 12: Red-pen annotation overlay

**Files:**
- Create: `packages/ui/src/annotations/{index.ts,types.ts,geometry.ts,roughen.ts,annotation-layer.tsx}`, `packages/ui/test/annotations.test.tsx`, `packages/ui/test/geometry.test.ts`
- Modify: `packages/ui/src/board/board.tsx` (render `AnnotationLayer` above the pieces and below `SquareGrid`), `apps/web/src/app/styleguide/sections/annotations.tsx`

**Interfaces:**
- Produces:
  - `type Tone = "pen" | "good" | "pencil"`.
  - `type Annotation = { kind: "arrow"; from: Square; to: Square; tone: Tone } | { kind: "circle"; square: Square; tone: Tone } | { kind: "mark"; square: Square; glyph: "??" | "?" | "?!" | "!" | "!!"; tone: Tone }`.
  - `AnnotationLayer` with props `{ annotations: Annotation[]; orientation: "white" | "black"; animate?: boolean }`. It is an SVG in `viewBox="0 0 8 8"`, `aria-hidden`, followed by a visually hidden `<ul>` describing each mark: `"Red arrow from e4 to e6"`, `"Green circle on f7"`, `"Red ?? on e6"`.
  - `squareCenter(sq: Square, orientation): { x: number; y: number }`.
  - `arrowGeometry(from: Square, to: Square, orientation): { shaft: string; head: string }`. A knight move gives an L-shaped shaft.
  - `roughen(d: string, seed: string, amount?: number): string`. It is deterministic: the same input always gives the same output, so screenshots are stable.
  - `circlePath(sq: Square, orientation, seed: string): string`. It starts at about 1 o'clock and sweeps 380°.
  - The draw-on is a Motion `motion.path` going from `pathLength` 0 to 1 over `dur.draw` with `ease.pen`. Each annotation is delayed `i * stagger`, and the arrowhead starts at 85% of its shaft. With `animate=false` or reduced motion, everything renders at `pathLength` 1 with no transition.

- [ ] **Step 1: Write failing tests.**
  - `squareCenter("a1", "white")` is `{ x: 0.5, y: 7.5 }`, and with `"black"` it is `{ x: 7.5, y: 0.5 }`.
  - `arrowGeometry("g1", "f3", "white").shaft` has exactly 2 line segments (an L shape), and `arrowGeometry("e2", "e4", ...)` has 1.
  - `roughen(d, "e4-e6")` called twice gives the same string, and `roughen(d, "e4-e5")` differs.
  - With reduced motion, every rendered `path` has no `initial` `pathLength` 0. The test checks the rendered `stroke-dashoffset` is 0.
  - The hidden list reads `["Red arrow from e4 to e6", "Red ?? on e6"]` for that input.
  - Every stroke is drawn twice, the halo first (`stroke` = `var(--color-sheet)`).
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement. The stroke width and arrowhead geometry come from the Task 10 decision.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** The styleguide section shows `middlegameBlunder` at its blunder: the played move as a pen arrow, the best move as a good arrow, a pen circle on the target square, and a `??` mark. It also has a "Replay marks" button. Run CL on `/styleguide` (board and annotations) and on `/lab/board/<chosen>`. The critic also gets a 12-frame strip of the draw-on, captured by `pnpm shots --motion --routes /styleguide#annotations`.
- [ ] **Step 6:** Commit `feat(ui): red-pen annotation overlay with draw-on motion`.

---

## W4: Charts and data display

Each W4 task loads the `dataviz` skill before writing chart code. The four tasks touch disjoint files and run in parallel once Tasks 3 and 5 have landed.

### Task 13: Eval graph (visx)

**Files:**
- Create: `packages/ui/src/eval-graph/{index.ts,eval-graph.tsx,scales.ts}`, `packages/ui/test/eval-graph.test.tsx`
- Modify: `apps/web/src/app/styleguide/sections/eval-graph.tsx`

**Interfaces:**
- Produces:
  - `EvalGraph` with props `{ points: { ply: number; winWhite: number; san: string; judgement?: "inaccuracy" | "mistake" | "blunder" | null }[]; label: string; currentPly?: number; onSelectPly?: (ply: number) => void; phases?: { middlegameStartPly?: number; endgameStartPly?: number }; playerColor?: "white" | "black"; height?: number /* default 160 */; drawIn?: boolean }`.
  - The White win% line is drawn in `ink` around a 50% `rule` baseline, with the area tinted `ink` at 8% above and below the baseline. Mistakes and blunders get `pen` ticks with glyphs, and inaccuracies get `pencil` ticks. Phase boundaries are `rule-strong` verticals labelled "Middlegame" and "Endgame" in sentence case. The current ply is a `pencil` cursor.
  - The graph is keyboard-operable as `role="slider"`, with `aria-valuemin=1` and `aria-valuemax=<last ply>`. `aria-valuetext` reads like `"Move 31, White Rxe6, blunder, White 22%"`.
  - `drawIn` reveals left to right with a clip over `2 × dur.reveal`. With reduced motion there is no reveal.
  - `xForPly(ply, width, count)` and `yForWin(win, height)` live in `scales.ts`.

- [ ] **Step 1: Write failing tests.**
  - `yForWin(50, 160)` is 80, `yForWin(100, 160)` is 0, and `yForWin(0, 160)` is 160.
  - With `middlegameBlunder`, the number of pen ticks equals the number of plies judged `mistake` or `blunder`.
  - ArrowRight with `currentPly: 30` calls `onSelectPly(31)`, End calls it with the last ply, and Home calls it with 1.
  - The `aria-valuetext` string matches exactly for ply 31 of the fixture (computed from the fixture).
  - `renders 400 plies`: a synthetic 400-point series renders one path, and every point's x stays within the width at 320px.
  - With 0 points, it renders `ProgressPlaceholder`-style text `"No moves to graph yet"` and no slider.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement with `@visx/shape`, `@visx/scale` and `@visx/responsive`.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** The styleguide section shows all three fixture games, one of them linked to a `Board` through `currentPly` (if Task 11 has landed; otherwise a notation readout). Run CL on `/styleguide` (eval graph).
- [ ] **Step 6:** Commit `feat(ui): eval graph with move-quality ticks and keyboard scrubbing`.

### Task 14: Sparkline, KPI with confidence band, and NumberFlow stat

**Files:**
- Create: `packages/ui/src/stats/{index.ts,sparkline.tsx,kpi-stat.tsx,stat.tsx}`, `packages/ui/test/stats.test.tsx`
- Modify: `apps/web/src/app/styleguide/sections/stats.tsx`

**Interfaces:**
- Produces:
  - `Sparkline` with props `{ values: (number | null)[]; label: string; highlightLast?: boolean }`. It has no axes, nulls break the line, and the last point is an `ink` dot.
  - `Stat` with props `{ value: number; format: "percent" | "decimal" | "integer"; locale?: string }`. It wraps NumberFlow with tabular numerals, `animated={!reducedMotion}`, and `decimal` shows 2 places.
  - `KpiStat` with props `{ label: string; value: number; format: "percent" | "decimal" | "integer"; interval: { low: number; high: number; level: 0.8 | 0.9 }; basis: { games: number; timeClass: string }; provisional?: boolean; trend?: (number | null)[]; target?: { value: number; label: string } }`.
    - It renders `Stat`, then the interval as a `rule-strong` band on a short scale with the value tick in `ink` and the target tick in `good`.
    - The basis line reads `"Based on 38 rapid games"`, and the band is described as `"80% likely between 61.0 and 67.5"`.
    - `provisional` shows the label `"Early estimate"`.

- [ ] **Step 1:** Write failing tests:
  - `Sparkline` with `[1, 2, 3]` renders one path with 3 points. With `[1, null, 3]` it renders 2 subpaths. With fewer than 2 numbers it renders `aria-label="Not enough games yet"` and no path.
  - `KpiStat` renders `"Based on 38 rapid games"` and `"80% likely between 61.0 and 67.5"` for `interval { low: 61, high: 67.5, level: 0.8 }`, `format: "decimal"`.
  - `provisional` shows `"Early estimate"`.
  - `Stat` renders `"64.2%"` for `0.642` percent (NumberFlow's text content).
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Fill the section from `kpiFixtures`. Avoid a stat-banner row (B9): KPIs sit in a list alongside their sparklines. Run CL on `/styleguide` (stats).
- [ ] **Step 6:** Commit `feat(ui): sparkline, kpi with confidence band and animated stat`.

### Task 15: Area score display and weakness timeline swimlane

**Files:**
- Create: `packages/ui/src/area-score/{index.ts,area-score.tsx}`, `packages/ui/src/timeline/{index.ts,weakness-timeline.tsx,states.ts}`, `packages/ui/test/area-score.test.tsx`, `packages/ui/test/timeline.test.tsx`
- Modify: `apps/web/src/app/styleguide/sections/{area-score,timeline}.tsx`

**Interfaces:**
- Produces:
  - `type Area = "openings" | "middlegame" | "endgame" | "tactics" | "calculation" | "time" | "mindset"` and `areaLabels: Record<Area, string>` (`"Openings"`, `"Middlegame"`, `"Endgame"`, `"Tactics"`, `"Calculation"`, `"Time"`, `"Mindset"`).
  - `AreaScore` with props `{ area: Area; score: number /* 0–100 */; band: { median: number; next: number }; delta?: number; basis: { games: number; timeClass: string } }`. It is a ruler-style row: the score numeral in Literata, a `rule` scale with the player's mark in `ink`, the band median tick in `pencil`, and the next-band tick in `good`. Its accessible text reads like `"Endgame 62, band median 55, next band 68"`. `AreaScoreList` with props `{ items: AreaScoreProps[] }` is a list, not a card grid (B4).
  - `type WeaknessState = "not_an_issue" | "emerging" | "active" | "improving" | "resolved" | "relapsed"` and `stateStyle: Record<WeaknessState, { label: string; fill: string; pattern: "none" | "hatch" | "dots" }>`. The labels are `"Not an issue"`, `"Emerging"`, `"Active"`, `"Improving"`, `"Resolved"` and `"Relapsed"`. Each state differs by pattern as well as colour.
  - `WeaknessTimeline` with props `{ series: { id: string; label: string; segments: { state: WeaknessState; from: string; to: string | null }[]; onset?: { date: string; low: string; high: string } }[]; events?: { date: string; kind: "plan" | "drill"; label: string }[]; range: { from: string; to: string } }`. It has one swimlane per series. Onset is a `pen` tick with a whisker for its interval. It scrolls horizontally inside its own container below 640px, with sticky lane labels.

- [ ] **Step 1:** Write failing tests:
  - `AreaScoreList` with `areaScoreFixtures` renders the seven labels in order, and the accessible text matches for endgame.
  - A `delta` of `+4` renders `"+4 since last report"`.
  - The timeline legend lists the six labels.
  - A segment from 2026-09-01 to 2026-10-01 in a range of 2026-09-01 to 2026-11-01 has a width of 50% of the lane, within 0.5%.
  - `to: null` extends to `range.to`.
  - The onset whisker spans `low` to `high`.
  - `timeline scrolls inside its container`: the container has `overflow-x: auto` and the page wrapper doesn't.
  - 12 series render 12 lanes.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement. The patterns are SVG `<pattern>` elements built from tokens.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Fill both sections from the fixtures, then run CL on `/styleguide` (area score and timeline).
- [ ] **Step 6:** Commit `feat(ui): area score ruler and weakness timeline swimlanes`.

### Task 16: Progress placeholder

**Files:**
- Create: `packages/ui/src/progress/{index.ts,progress-placeholder.tsx}`, `packages/ui/test/progress.test.tsx`
- Modify: `apps/web/src/app/styleguide/sections/progress.tsx`

**Interfaces:**
- Produces: `ProgressPlaceholder` with props `{ shape: "graph" | "stat" | "list" | "board"; verb: "Analysing" | "Importing"; progress?: { done: number; total?: number; unit: string }; basisNote?: string }`.
  - It is a static skeleton in the shape of the section it stands in for: a ruled sheet with faint lines, never a spinner (B18).
  - It has `role="status"` and `aria-live="polite"`.
  - When the total is known, a pencil progress stroke grows with `done/total`. With reduced motion the stroke is static.

- [ ] **Step 1:** Write failing tests:
  - `{ verb: "Analysing", progress: { done: 38, total: 200, unit: "rapid games" } }` renders `"Analysing 38 / 200 rapid games"`.
  - Without a total, `{ verb: "Importing", progress: { done: 120, unit: "games" } }` renders `"Importing: 120 games so far"`.
  - Without progress it renders `"Analysing"` alone.
  - `role="status"` is present.
  - No element has an `animate-spin` class.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** The section shows all four shapes, then the real component swapping in, using the reveal motion for the same section. Run CL on `/styleguide` (progress).
- [ ] **Step 6:** Commit `feat(ui): designed progress placeholders`.

---

## W1: Auth, accounts, age gate, shell

### Task 17: Better Auth core, magic link via Mailpit, Google

**Files:**
- Create:
  - `apps/web/src/server/auth.ts`
  - `apps/web/src/server/auth/{session.ts,google.ts,oauth-providers.ts,lichess.ts,chesscom-oauth.ts,db-hooks.ts,hooks.ts}`. `oauth-providers.ts` is final here: `oauthProviders(env)` returns `[lichessProvider(env), chesscomOAuthProvider(env)].filter(Boolean)`. `lichess.ts` and `chesscom-oauth.ts` start as stubs that return `null`. Task 18 owns `lichess.ts`, Task 19 owns `chesscom-oauth.ts` and Task 20 owns `hooks.ts`, so Tasks 18–20 never edit the same file. `db-hooks.ts` and `hooks.ts` start as empty stubs.
  - `apps/web/src/server/mail.ts`, `apps/web/src/lib/auth-client.ts`, `apps/web/src/app/api/auth/[...all]/route.ts`
  - `apps/web/test/helpers/session.ts`, `apps/web/test/auth/core.test.ts`, `apps/web/e2e/auth.spec.ts`
- Modify: `packages/db/src/auth-schema.ts`, only if the Better Auth CLI diff requires it, plus a migration

**Interfaces:**
- Produces:
  - `auth` (betterAuth with the drizzle adapter, provider `"pg"`). Its settings:
    - `advanced.database.generateId: "uuid"` (confirm the option name with context7)
    - `advanced.cookiePrefix: "ci"`
    - `account.encryptOAuthTokens: true`; if that option doesn't exist, encrypt with AES-256-GCM in `db-hooks.ts` using a key derived from `BETTER_AUTH_SECRET`
    - `account.accountLinking: { enabled: true, trustedProviders: ["google"] }`
    - `rateLimit` with custom rule `"/sign-in/magic-link": { window: 600, max: 3 }`
    - plugins `magicLink({ expiresIn: 600, sendMagicLink })` and `genericOAuth({ config: oauthProviders(env) })`
    - `socialProviders: googleProvider(env)` (empty when the Google env vars are absent)
    - `hooks: authHooks`
    - `databaseHooks: authDbHooks`
    - `user.additionalFields` for the four age and privacy columns, all `input: false`
  - `getSession(headers: Headers): Promise<{ user: AppUser; session: { id: string; expiresAt: Date } } | null>`.
  - `requireUser(): Promise<AppUser>`, for server components. It redirects to `/sign-in` when there is no session, and to `/welcome/age` when `birthMonth` is null.
  - `requireApiUser(req: Request): Promise<AppUser | Response>`. It returns a `401 { error: "unauthenticated" }` or `403 { error: "age_required" }` Response.
  - `type AppUser = { id: string; email: string; name: string; birthMonth: string | null; marketingEmail: boolean; publicShareDefault: boolean }`.
  - `sendMail(msg: { to: string; subject: string; text: string; html: string }): Promise<void>` (nodemailer over `SMTP_URL`).
  - The magic-link email has subject `"Your sign-in link"` and text `"Use this link to sign in to chess-improver. It works once and expires in 10 minutes."` followed by the URL. The brand name comes from `brand.name`.
  - Test helper `createTestSession(db, overrides?: Partial<AppUser>): Promise<{ user: AppUser; cookie: string }>`. W5 uses it.
  - The web test DB is `WEB_DATABASE_URL_TEST`, guarded by `assertSafeTestDatabase` and migrated with `runMigrations`.

- [ ] **Step 1:** Run `pnpm dlx @better-auth/cli@1.7.27 generate --config apps/web/src/server/auth.ts --output <scratch>` and diff the output against `auth-schema.ts`. Expected: no differences except the declared additional fields. Fix the schema and add a migration if there are any.
- [ ] **Step 2: Write failing tests** (`core.test.ts`, run in the web container against `chess_web_test`):
  - Posting to `/api/auth/sign-in/magic-link` with `{ email: "ana@example.com" }` calls `sendMail` once with that recipient and a URL containing `/api/auth/magic-link/verify`.
  - Opening that URL creates a user with a uuid id and sets a cookie whose name starts with `ci.`.
  - `reused magic link is rejected`: a second verify with the same token doesn't create a session and redirects to `/sign-in?error=link_expired`.
  - A fourth magic-link request within 10 minutes returns 429.
  - `googleProvider({})` returns `{}`, and with both env vars set it returns a `google` key.
  - `requireApiUser` without a cookie returns 401. With a session whose `birthMonth` is null it returns 403 `age_required`.
- [ ] **Step 3:** Run `docker compose run --rm web pnpm --filter @ci/web test auth/core`. Expected: FAIL.
- [ ] **Step 4:** Implement.
- [ ] **Step 5:** Run the tests. Expected: PASS.
- [ ] **Step 6:** Write the e2e test `auth.spec.ts`, which runs against compose with Mailpit, `/sign-in` and the age page from Task 21. It is marked `test.fixme` until Task 21 lands. The flow:
  1. Fill the email and press "Email me a sign-in link". Expect the text "Check your email".
  2. Fetch `http://localhost:58025/api/v1/search?query=to:<email>` and open the link from the newest message.
  3. Expect `/welcome/age`.
- [ ] **Step 7:** Commit `feat(web): better auth with magic link via mailpit and google`.

### Task 18: Lichess OAuth provider and Lichess linking

**Files:**
- Create: `apps/web/src/server/accounts/lichess-link.ts`, `apps/web/test/fakes/{lichess.ts,chesscom.ts,main.ts}`, `apps/web/test/auth/lichess.test.ts`, `docs/decisions/ADR-WEB-4-lichess-oauth.md`
- Modify: `apps/web/src/server/auth/lichess.ts` (replaces the stub), `apps/web/src/server/auth/db-hooks.ts`

**Interfaces:**
- Produces:
  - `lichessProvider(env: WebEnv): GenericOAuthConfig` with:
    - `providerId: "lichess"`, `clientId: env.LICHESS_CLIENT_ID`, no client secret
    - `authorizationUrl: ${LICHESS_AUTHORIZE_BASE_URL}/oauth`, `tokenUrl: ${LICHESS_API_BASE_URL}/api/token`
    - `scopes: ["email:read"]`, `pkce: true`
    - `getUserInfo`, which fetches `/api/account` and `/api/account/email` and returns `{ id: account.id, name: account.username, email, emailVerified: false, image: null }`
  - `onLichessAccount(db, { userId, lichessId, username }): Promise<void>`, called from `databaseHooks.account.create.after` for `providerId === "lichess"`. It upserts `linked_accounts` (`platform: "lichess"`, `username` lower-cased, `displayName` = username) and an `accountLinks` row (`method: "lichess_oauth"`, `verifiedAt: now`). If another user held the verified link, it clears that user's `verifiedAt`.
  - `lichessLinkStatus(account: { accessTokenExpiresAt: Date | null }, now: Date): "ok" | "expiring" | "expired"`. It returns `"expiring"` within 14 days of expiry. Lichess has no refresh token, so the UI offers "Reconnect Lichess".
  - Fake servers (`startFakes(port): Promise<{ url: string; close(): Promise<void>; lastTokenRequest(): URLSearchParams | null }>`):
    - `/oauth` redirects straight back to `redirect_uri` with `code` and `state`, or with `error=access_denied` when `login_hint=deny`.
    - `/api/token` checks that `code_verifier` hashes (S256) to the stored `code_challenge`, then returns `{ token_type: "Bearer", access_token: "lio_test_token", expires_in: 31536000 }`.
    - `/api/account` and `/api/account/email` return fixed values.
    - `/chesscom/pub/player/:u` serves profiles from an in-memory map. `POST /chesscom/__set-location` with `{ username, location }` lets tests set a location, and an unknown username returns 404.
    - `main.ts` serves all of these on port 3199 for the compose `fakes` service.

- [ ] **Step 1: Write failing tests** (`lichess.test.ts`, in-process against `auth.handler`, with the fake started on a random port):
  - The sign-in redirect URL has `code_challenge_method=S256`, `client_id=chess-improver`, `scope=email:read`, a `state`, and a `code_challenge` of 43 characters.
  - After the callback, the fake's `lastTokenRequest()` includes `code_verifier` and `client_id`, and contains no `client_secret` (or an empty one).
  - A new user exists with the fake email. The `account` row has `providerId: "lichess"`, and its stored `access_token` isn't `"lio_test_token"` (it is encrypted at rest).
  - `accessTokenExpiresAt` is within a minute of now + 365 days.
  - `linked_accounts` has `platform: "lichess"` and `username: "fakeuser"`, and `accountLinks` has `method: "lichess_oauth"` with `verifiedAt` set.
  - Linking while signed in as an existing magic-link user (`linkSocial`) adds the link to that user and creates no new user.
  - `email collision is not auto-linked`: signing in with Lichess when a magic-link user already has the same email creates no new account row and redirects to `/sign-in?error=account_not_linked`.
  - `consent denied returns to sign-in with message`: the `error=access_denied` callback redirects to `/sign-in?error=lichess_denied`.
  - `lichessLinkStatus` returns `"ok"` 30 days before expiry, `"expiring"` 10 days before, and `"expired"` 1 day after.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement. ADR-WEB-4 covers: `email:read` (Better Auth needs an email), not trusting Lichess emails for auto-linking, the expiry and reconnect handling, and the split between the authorize and API base URLs for e2e.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(web): lichess oauth pkce provider and lichess account linking`.

### Task 19: chess.com linking and location-code verification

**Files:**
- Create: `apps/web/src/server/chesscom/profile.ts`, `apps/web/src/server/accounts/{chesscom-link.ts,list.ts,remove.ts}`, `apps/web/src/app/api/accounts/route.ts`, `apps/web/src/app/api/accounts/chesscom/route.ts`, `apps/web/src/app/api/accounts/chesscom/[linkId]/verify/route.ts`, `apps/web/src/app/api/accounts/[linkId]/route.ts`, `apps/web/test/accounts/chesscom.test.ts`, `docs/decisions/ADR-WEB-5-chesscom-verification.md`
- Modify: `apps/web/src/server/auth/chesscom-oauth.ts` (replaces the stub; it returns a config only when `CHESSCOM_OAUTH_ENABLED` is set, and `null` otherwise)

**Interfaces:**
- Produces:
  - `fetchChesscomProfile(username: string, deps: { fetch: typeof fetch; baseUrl: string; contactEmail: string; timeoutMs?: number }): Promise<{ username: string; location: string | null } | null>`.
    - The request sends `User-Agent: chess-improver/0.1 (+mailto:<CONTACT_EMAIL>)`.
    - It returns `null` on 404.
    - It throws `ChesscomUnavailableError` on a 403, 429, 5xx or timeout (default 5000 ms).
  - `generateVerificationCode(rng?: () => number): string`. The format is `CI-` plus 6 characters from `23456789ABCDEFGHJKMNPQRSTUVWXYZ`.
  - `startChesscomLink(deps, userId: string, username: string): Promise<{ linkId: string; username: string; code: string; verified: false }>`.
    - It validates `^[A-Za-z0-9_-]{3,25}$` and lower-cases the name.
    - It checks the profile exists.
    - It upserts `linked_accounts` (`platform: "chesscom"`) and `accountLinks` (`method: "username"`, unverified), plus `chesscomVerifications` (`expiresAt` = now + 7 days).
    - It allows at most 3 chess.com links per user.
    - It is idempotent per user and username, returning the same code.
  - `checkChesscomVerification(deps, userId: string, linkId: string, now: Date): Promise<{ status: "verified" | "pending" | "expired"; nextCheckAt: Date }>`.
    - Within 60 s of `lastCheckedAt`, it returns `pending` without fetching.
    - The match is case-insensitive: the code appears anywhere in the trimmed `location`.
    - On success it sets `verifiedAt` and `method: "location_code"`, and clears any other user's verified link to the same account in the same transaction.
  - `listAccounts(db, userId): Promise<{ linkId: string; platform: "lichess" | "chesscom"; username: string; displayName: string; verified: boolean; status: "ok" | "expiring" | "expired" | "pending_verification"; code?: string }[]>`.
  - `removeLink(db, userId, linkId): Promise<void>`. It deletes only the user's own `accountLinks` row; the shared `linked_accounts` and `games` rows stay.
  - Routes:
    - `GET /api/accounts` → 200 with the list.
    - `POST /api/accounts/chesscom` `{ username }` → 201, or 400 `{ error: "invalid_username" }`, 404 `{ error: "not_found" }`, 409 `{ error: "limit_reached" }`, or 503 `{ error: "chesscom_unavailable" }`.
    - `POST /api/accounts/chesscom/:linkId/verify` → 200.
    - `DELETE /api/accounts/:linkId` → 204.
  - When `CHESSCOM_ENABLED` is false, every chess.com route returns 404.
  - `chesscomOAuthProvider(env)` uses OIDC discovery `https://oauth.chess.com/.well-known/openid-configuration` and is registered only when `CHESSCOM_OAUTH_ENABLED` is set and the client id and secret are present. Its endpoints are unverified until chess.com approves the application (§18.1).

- [ ] **Step 1: Write failing tests** (real DB, fake fetch):
  - The User-Agent header matches `chess-improver/0.1 (+mailto:dev@example.invalid)`.
  - A 404 gives `null`, and a 429 throws `ChesscomUnavailableError`.
  - A code matches `/^CI-[2-9A-HJKMNP-Z]{6}$/`.
  - `startChesscomLink` with `"Hikaru"` stores `"hikaru"`, and a second call returns the same code.
  - `"ab"` gives `invalid_username`, and a 4th link gives `limit_reached`.
  - Verify while the code isn't in `location` gives `pending`. Verify again within 60 s makes 0 fetch calls. Once `location` is `"Paris CI-7K3QXM "`, verify gives `verified`.
  - `unverified links do not block others`: users A and B both start links to `"hikaru"` and both succeed.
  - `newest verified owner wins`: A verifies, then B puts its code in the location and verifies. B is verified and A's `verifiedAt` is null.
  - After 7 days, verify gives `expired`.
  - `CHESSCOM_ENABLED=false` makes `POST /api/accounts/chesscom` return 404.
  - `oauthProviders(env)` has no `chesscom` entry by default.
  - `removeLink` leaves the `linked_accounts` row in place.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement. ADR-WEB-5 covers the code format, the 60-second cooldown, the 7-day expiry, the newest-verification-wins rule, and why the check runs on demand (one profile request, not engine work; a background recheck is M7).
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(web): chess.com linking with location-code verification behind CHESSCOM_ENABLED`.

### Task 20: Age gate and privacy defaults

**Files:**
- Create: `apps/web/src/server/age.ts`, `apps/web/src/app/api/age/route.ts`, `apps/web/test/age/age.test.ts`, `docs/decisions/ADR-WEB-6-age-gate.md`
- Modify: `apps/web/src/server/auth/hooks.ts`

**Interfaces:**
- Produces:
  - `parseBirthMonth(input: { year: number; month: number }, today: Date): string`. It returns `"YYYY-MM"` and throws `InvalidBirthMonthError` for a month in the future, a year before 1900, or a month outside 1–12.
  - `ageFromBirthMonth(birthMonth: string, today: Date): number`. It is conservative: it assumes the birthday falls on the last day of the month, so it never overstates the age.
  - `ageBand(age: number): "under13" | "teen" | "adult"`.
  - `isMinor(user: AppUser, today: Date): boolean`.
  - `canEnableMarketing(user: AppUser, today: Date): boolean`. It returns false for minors.
  - `recordAge(deps: { db }, userId: string, input: { year: number; month: number }, now: Date): Promise<{ band: "under13" | "teen" | "adult"; redirectTo: "/app/accounts" | "/blocked" }>`.
    - Under 13: it deletes the user, which cascades sessions, OAuth accounts and links.
    - Teen: it sets `birthMonth`, `ageCheckedAt`, `marketingEmail: false` and `publicShareDefault: false`.
    - Adult: it sets `birthMonth` and `ageCheckedAt`. Both preferences stay `false` as opt-in defaults.
  - `POST /api/age` `{ year, month }`, for a signed-in user.
    - It responds 200 `{ redirectTo }`.
    - For under-13s it also sets `ci_age_block=1; Max-Age=2592000; Path=/; HttpOnly; SameSite=Lax; Secure` (no `Secure` in dev) and clears the session cookie.
  - The `authHooks.before` matcher covers `/sign-in/*`, `/sign-up/*`, `/magic-link/*` and `/oauth2/*`. When the request carries `ci_age_block`, it throws `APIError("FORBIDDEN", { code: "AGE_BLOCKED" })`.

- [ ] **Step 1: Write failing tests**, with today = 2026-11-21:
  - `ageFromBirthMonth("2013-11")` is 12, `("2013-10")` is 13, `("2008-11")` is 17 and `("2008-10")` is 18.
  - `ageBand(12)` is `"under13"`, `ageBand(13)` is `"teen"` and `ageBand(18)` is `"adult"`.
  - `parseBirthMonth({ year: 2026, month: 12 })` throws, as does `parseBirthMonth({ year: 1899, month: 5 })`.
  - `recordAge` for 2014-01:
    - The band is `under13`, `redirectTo` is `"/blocked"`, and the user, sessions and `accountLinks` rows are gone.
    - The shared `linked_accounts` rows remain, and the email no longer exists in `user`.
  - `recordAge` for 2010-03 gives `teen`, with `marketingEmail` false and `publicShareDefault` false, and `canEnableMarketing` returns false.
  - A request to `/api/auth/sign-in/magic-link` with the cookie `ci_age_block=1` returns 403 with code `AGE_BLOCKED` and sends no mail.
  - `POST /api/age` for under-13s sets `ci_age_block` with `Max-Age=2592000`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement. ADR-WEB-6 covers:
  - The neutral, post-auth age screen, which doesn't announce the threshold in advance.
  - Month and year only (data minimisation).
  - The conservative age arithmetic.
  - Deleting a blocked user's data immediately.
  - The 30-day block cookie, which is essential and stops retries with the back button or another provider.
  - No cookie banner, because only essential cookies are set.
  - The DPIA and the nudge review stay in M7.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(web): age gate with under-13 block and teen privacy defaults`.

### Task 21: App shell, sign-in, age and account-linking screens

**Files:**
- Create:
  - `apps/web/src/app/(auth)/sign-in/{page.tsx,sign-in-form.tsx}`, `apps/web/src/app/(auth)/sign-in/check-email/page.tsx`
  - `apps/web/src/app/(auth)/welcome/age/{page.tsx,age-form.tsx}`, `apps/web/src/app/(auth)/blocked/page.tsx`
  - `apps/web/src/app/(app)/layout.tsx`, `apps/web/src/app/(app)/app/accounts/{page.tsx,accounts-view.tsx,chesscom-link-form.tsx}`
  - `apps/web/src/app/(app)/shell/{header.tsx,user-menu.tsx,theme-toggle.tsx,nav-command-menu.tsx}`
  - `apps/web/e2e/accounts.spec.ts`, `packages/ui/qa/log/auth-and-accounts.md`
- Modify: `apps/web/src/app/page.tsx` (redirects to `/app/accounts` when signed in, otherwise to `/sign-in`), `apps/web/e2e/auth.spec.ts` (removes the `fixme`)

**Interfaces:**
- Consumes: Tasks 7, 8 and 17–20.
- Produces these routes and their exact copy:
  - `/sign-in`:
    - Heading `"Sign in"`.
    - An email field labelled `"Email"` with the button `"Email me a sign-in link"`, then `"Continue with Lichess"`, then `"Continue with Google"` (shown only when it is configured).
    - Error messages by query parameter:
      - `link_expired`: `"This link has expired. Request a new one."`
      - `lichess_denied`: `"Lichess sign-in was cancelled. You can try again or use your email."`
      - `account_not_linked`: `"That email already has an account. Sign in with your email link, then connect Lichess from Accounts."`
    - With `ci_age_block` set, the page shows the blocked notice instead of the form.
  - `/sign-in/check-email`: `"Check your email"` and `"We sent a sign-in link to <email>. It expires in 10 minutes."`
  - `/welcome/age`: `"When were you born?"` with month and year selects, the hint `"We ask so we can keep younger players' accounts private by default."` and the button `"Continue"`.
  - `/blocked`: `"You need to be 13 or older to use chess-improver."` and `"We've deleted the details you gave us."`
  - `/app/accounts`:
    - Heading `"Accounts"`. When there are no accounts: `"Connect a Lichess or chess.com account to import your games."`
    - Lichess: `"Connect Lichess"` gives the toast `"Lichess connected"`. When the link is expiring or expired, the row shows `"Reconnect Lichess"`.
    - chess.com (only when `CHESSCOM_ENABLED`): a field labelled `"chess.com username"` with the button `"Connect chess.com"`, giving the toast `"chess.com connected"`.
    - A pending row shows `"Add <code> to the Location field on your chess.com profile, then check again. chess.com can take up to 12 hours to show profile changes."`, a copy-code button `"Copy code"` and `"Check again"`. A pending result shows `"Not there yet. Try again in a minute."` A verified result gives the toast `"chess.com account verified"`.
    - `"Remove"` opens a dialog: `"Remove <username>?"` / `"Your imported games stay, but they won't sync to your account."` / `"Remove account"`.
  - The shell header has the `brand.wordmark`, a nav item `"Accounts"`, the theme toggle (`"Use dark theme"` / `"Use light theme"`), a command menu trigger (`Kbd` Ctrl K), and a user menu with `"Sign out"`. It leaves a slot `data-slot="activity"` for Task 27.

- [ ] **Step 1: Write the failing e2e test** `accounts.spec.ts`. It runs with `docker compose -f docker-compose.yml -f docker-compose.e2e.yml --profile e2e up -d`. The flow:
  1. Sign in by magic link and enter an adult age.
  2. See the empty state.
  3. Connect chess.com `fakeplayer` and see the pending instructions with a code.
  4. Set the fake profile's location to the code through the fake's test endpoint, then press "Check again" and see "chess.com account verified".
  5. Run "Continue with Lichess" from a fresh context through the fake, and land on `/welcome/age`.
  6. Remove the chess.com account and see the empty state again.

  It also covers the under-13 path: 2015-01 lands on `/blocked`, and revisiting `/sign-in` shows the blocked notice.
- [ ] **Step 2:** Run `pnpm e2e accounts auth`. Expected: FAIL.
- [ ] **Step 3:** Implement. Server components call `requireUser()`. Client mutations use TanStack Query (`queryKeys.accounts()` from Task 23; until Task 23 lands, inline `["accounts"]`, which Task 27 swaps). Every async action shows a pending state on its button, never a blocking spinner.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Run CL on `/sign-in`, `/sign-in/check-email`, `/blocked`, and `/app/accounts` in its empty, pending and verified states. Shots of authenticated pages use the storage state from the e2e run: `pnpm shots --storage apps/web/e2e/.auth/adult.json`.
- [ ] **Step 6:** Commit `feat(web): app shell, sign-in, age and account linking screens`.

---

## W5: Realtime

### Task 22: Event hub and `/api/events` (SSE, replay, polling)

**Files:**
- Create: `apps/web/src/server/events/{hub.ts,sse.ts,user-accounts.ts}`, `apps/web/src/app/api/events/route.ts`, `apps/web/test/events/{sse-format.test.ts,route.test.ts,helpers.ts}`, `docs/decisions/ADR-WEB-8-sse-delivery.md`

**Interfaces:**
- Consumes: `userEventsChannel`, `listEventsAfter`, `oldestEventId` and `StreamEvent` (Task 2); `requireApiUser` and `createTestSession` (Task 17); the M1 pub/sub message shape `{ id, type, payload }`.
- Produces:
  - `createEventHub(deps: { subscriber: Redis }): { subscribe(accountIds: string[], listener: (e: StreamEvent) => void): () => void; channelCount(): number }`. There is one Redis subscriber per server process. Channels are refcounted, and `accountId` is filled in from the channel name.
  - `getEventHub(): EventHub` is a lazy process singleton.
  - `userAccountIds(db, userId): Promise<string[]>` returns the user's `accountLinks` account ids, verified or not.
  - `formatSseEvent(e: StreamEvent): string`, which returns `id: <id>\nevent: <type>\ndata: {"accountId":"<id>","payload":<json>}\n\n`.
  - `GET /api/events` (`runtime = "nodejs"`, `dynamic = "force-dynamic"`):
    - **Poll mode:** `?after=<id>` returns JSON `{ events: StreamEvent[]; lastId: number; reset: boolean }`, with a limit of 500 and `reset: true` when `after` is older than the oldest retained id.
    - **SSE mode** (otherwise): the headers are `Content-Type: text/event-stream`, `Cache-Control: no-cache, no-transform` and `X-Accel-Buffering: no`, and the stream opens with `retry: 3000`. The cursor is the `Last-Event-ID` header, or else `?since=`.
      1. Subscribe to the hub and buffer live events.
      2. If there is a cursor older than the oldest retained event, write `event: reset` (data `{}`). Otherwise replay `listEventsAfter(cursor)` in order.
      3. Flush the buffer, dropping ids ≤ the last id sent.
      4. Then forward live events, dropping any id ≤ the last id sent.
      5. With no cursor, write `event: hello` with `id: <max id or 0>`.
      6. Send a heartbeat `: ping` every 15000 ms. On `request.signal` abort, unsubscribe and clear the timer.
    - Unauthenticated requests get 401, and age-pending users get 403.

- [ ] **Step 1: Write failing tests.**
  - `sse-format.test.ts`: `formatSseEvent({ id: 42, accountId: "a", type: "game.analysed", payload: { gameId: "g1", accuracy: 81.5 } })` equals `'id: 42\nevent: game.analysed\ndata: {"accountId":"a","payload":{"gameId":"g1","accuracy":81.5}}\n\n'`.
  - `route.test.ts` (real Postgres and Redis; the helper `insertAndPublish(db, redis, accountId, type, payload)` mirrors M1 `publishEvent`):
    - `replay plus live race delivers each id once in order`:
      1. Insert events e1–e5 for the user's account.
      2. Open the stream with `Last-Event-ID: e2`.
      3. Using a hub test hook that fires before the replay query, publish e6.
      4. Read frames until e6 arrives. The ids are exactly `[e3, e4, e5, e6]`.
    - `delivers live events after replay`: publish e7 and receive e7.
    - `does not leak other users' events`: an event on another user's account never arrives within 300 ms.
    - `stale cursor emits reset`: delete the rows ≤ e3 and connect with `Last-Event-ID: e1`. The first event frame is `event: reset`.
    - `no cursor sends hello with current max id`.
    - `?after=e3` returns JSON with ids `[e4, e5, ...]` and `reset: false`.
    - `401 without session`.
    - `abort unsubscribes`: after aborting the request, `channelCount()` returns 0.
- [ ] **Step 2:** Run `docker compose run --rm web pnpm --filter @ci/web test events`. Expected: FAIL.
- [ ] **Step 3:** Implement. ADR-WEB-8 covers: subscribe-then-replay with id dedupe, the `reset` event for cursors past the 7-day retention (pruning is a worker job in a later milestone), the poll format, a shared subscriber per process, and that an account linked while the stream is open is picked up on the client's next reconnect.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(web): sse event stream with last-event-id replay and polling`.

### Task 23: Client stream hook, cache patching, polling fallback, activity indicator

**Files:**
- Create: `apps/web/src/lib/query-keys.ts`, `apps/web/src/lib/realtime/{use-event-stream.ts,reducers.ts,activity.ts,realtime-provider.tsx,query-provider.tsx}`, `apps/web/src/components/activity-indicator.tsx`, `apps/web/test/events/{reducers.test.ts,use-event-stream.test.tsx,activity-indicator.test.tsx}`

**Interfaces:**
- Produces:
  - `queryKeys = { accounts: () => ["accounts"] as const, importProgress: (accountId: string) => ["import-progress", accountId] as const, games: (accountId: string) => ["games", accountId] as const, game: (gameId: string) => ["game", gameId] as const, activity: () => ["activity"] as const }`.
  - `type Activity = { imports: Record<string, { imported: number; total?: number }>; failures: { kind: string; reason: string; at: number }[] }`.
  - `applyEvent(qc: QueryClient, e: StreamEvent): void`, which maps each event type as follows:

    | Event | Cache change |
    |---|---|
    | `import.progress` | Sets `importProgress(accountId)` to the payload and updates `activity.imports[accountId]`. |
    | `game.analysed` | Patches `game(gameId)` if it is cached (`accuracy`, `analysisStatus: "done"`) and invalidates only `games(accountId)` with `exact: true`. |
    | `job.failed` | Appends to `activity.failures` and shows `toast` text `"A <kind> job failed: <reason>"` with no modal. |
    | `reset` | Calls `qc.invalidateQueries()`, the only invalidation that touches everything. |
    | unknown | Ignored. |

  - `useEventStream(opts?: { enabled?: boolean; reconnectKey?: string }): { mode: "sse" | "polling" | "off"; lastEventId: number }`.
    - It uses EventSource on `/api/events`.
    - It falls back to polling `GET /api/events?after=<lastEventId>` every 5000 ms when `EventSource` is undefined, or after 3 consecutive `error`s before any `open`.
    - It ignores any event with an id ≤ `lastEventId`.
    - It reopens when `reconnectKey` changes; the accounts list hash is passed in.
  - `QueryProvider`, and `RealtimeProvider` (for signed-in users, it mounts `useEventStream` with `reconnectKey` = the joined account ids).
  - `ActivityIndicator` reads `activity`:
    - While any import is unfinished it shows `"Importing 120 / 400 games"`, or `"Importing: 120 games so far"` when the total is unknown.
    - It shows `"3 jobs failed"` as a button that opens a popover listing them.
    - It renders nothing when idle.

- [ ] **Step 1:** Write failing tests (jsdom, a fake `EventSource` class, fake timers):
  - `game.analysed` patches a cached `["game", "g1"]` to `accuracy: 81.5`, invalidates `["games", "a1"]`, and leaves `["games", "a2"]` and `["accounts"]` alone.
  - `reset` invalidates everything.
  - Duplicate id 7 delivered twice is applied once.
  - Three errors without an open switch `mode` to `"polling"`, and the next tick fetches `/api/events?after=<last>`.
  - With `EventSource` undefined, the mode starts as `"polling"`.
  - Polling advances `lastEventId` to the response's `lastId`.
  - `ActivityIndicator` renders `"Importing 120 / 400 games"`, then `"3 jobs failed"`, and nothing when idle.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(web): event stream hook with cache patching, polling fallback and activity indicator`.

---

## W6: Visual QA tooling

### Task 24: Playwright, `scripts/shots.ts`, Playwright MCP, lab pattern, critique template

**Files:**
- Create: `apps/web/playwright.config.ts`, `scripts/shots.ts`, `scripts/shots-lib.ts`, `scripts/test/shots.test.ts`, `.mcp.json`, `apps/web/src/app/lab/{layout.tsx,page.tsx,lab-frame.tsx,lab-enabled.ts}`, `apps/web/test/lab.test.ts`, `packages/ui/qa/critique-prompt.md`, `packages/ui/qa/log/README.md`, `docs/decisions/ADR-WEB-10-visual-qa-loop.md`

**Interfaces:**
- Produces:
  - Playwright projects:
    - `e2e`: Chromium at 1280×800.
    - `visual-390-light`, `visual-390-dark`, `visual-1440-light`, `visual-1440-dark`: 390×844 and 1440×900, with `colorScheme` set and `reducedMotion: "reduce"`.

    `baseURL` comes from `E2E_BASE_URL`, default `http://localhost:3100`.
  - `parseShotsArgs(argv: string[]): { routes: string[]; baseUrl: string; out: string; storage?: string; motion: boolean }`. The default routes are `["/styleguide", "/sign-in", "/lab"]`, the default output is `.shots`, and `--routes` takes a comma list.
  - `shotPath(out: string, route: string, width: 390 | 1440, theme: "light" | "dark"): string`. For example, `/lab/board/a` at 390 light becomes `.shots/lab-board-a/390-light.png`, `/` becomes `home`, and a `#hash` is appended to the slug as `--hash`.
  - The shots script:
    - waits for `networkidle` and `document.fonts.ready`;
    - hides `[data-lab-chrome]`;
    - takes full-page captures;
    - with `--motion`, writes a 12-frame strip at 100 ms intervals instead of using reduced motion;
    - prints one line per file and exits with code 1 if any route returns ≥ 400.
  - `.mcp.json`, exactly:

    ```json
    { "mcpServers": { "playwright": { "command": "cmd", "args": ["/c", "npx", "-y", "@playwright/mcp@0.0.86", "--isolated", "--viewport-size=1440,900"] } } }
    ```
  - `labEnabled(env: { NODE_ENV?: string; LAB_ENABLED?: string }): boolean` returns `NODE_ENV !== "production" || LAB_ENABLED === "1"`. `/lab/layout.tsx` calls `notFound()` when it is false and sets `robots: { index: false }`.
  - `/lab` lists `styleguide-hero` and `board`, each with variants a, b and c.
  - `LabFrame` with props `{ surface: string; variant: "a" | "b" | "c"; title: string; rationale: string; children }`. It renders a variant switcher and the rationale in a `data-lab-chrome` footer.
  - `critique-prompt.md` tells the critic:
    - Inputs: screenshots, DESIGN.md and REFERENCES.md. It must not read source code.
    - Judge against the ban list, the tokens and the quality floor (360px, focus, contrast, reduced motion), and check the copy rules.
    - Output at most 10 ranked issues, each `{ rank, severity: "blocker" | "major" | "minor", banId?, screenshot, observation, fix }`, plus `verdict: "pass" | "revise"`. `pass` is allowed only with no blocker or major issues.

- [ ] **Step 1:** Write failing tests:
  - `shots.test.ts`: the `shotPath` cases above; `parseShotsArgs(["--routes", "/a,/b"]).routes` equals `["/a", "/b"]`; the matrix for 2 routes gives 8 paths.
  - `lab.test.ts`: `labEnabled({ NODE_ENV: "production" })` is false, `labEnabled({ NODE_ENV: "production", LAB_ENABLED: "1" })` is true, and `labEnabled({ NODE_ENV: "development" })` is true.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement, then run `pnpm exec playwright install chromium` on the host.
- [ ] **Step 4:** Run the tests, then `pnpm shots --routes /styleguide`. Expected: PASS, and 4 PNGs under `.shots/styleguide/`.
- [ ] **Step 5:** Verify the MCP: run `node -e "JSON.parse(require('fs').readFileSync('.mcp.json','utf8'))"`, then start a new Claude Code session, run `/mcp`, and check that `playwright` lists its tools. If it shows no tools, run `npx clear-npx-cache` and retry, and record what happened in ADR-WEB-10.
- [ ] **Step 6:** Commit `feat(qa): playwright projects, shots script, playwright mcp, lab pattern and critique template`.

### Task 25: Ban lint and third-party design tooling review

**Files:**
- Create: `scripts/ban-lint.ts`, `scripts/ban-rules.ts`, `scripts/test/ban-lint.test.ts`, `.claude/skills/web-design-guidelines/SKILL.md` (vendored only if the review passes), `docs/decisions/ADR-WEB-9-third-party-design-tooling.md`

**Interfaces:**
- Produces:
  - `banRules: { id: string /* B-id */; description: string; test(file: { path: string; text: string }): { line: number; match: string }[] }[]`.
  - `runBanLint(root: string): Promise<{ findings: { rule: string; path: string; line: number; match: string }[] }>`. It scans `apps/web/src` and `packages/ui/src`, and the CLI prints `0 findings` or the list, exiting with code 1 when there are findings.
  - The rules:

    | Rule | Flags |
    |---|---|
    | `B12` | imports of `chessground`, `@lichess-org/chessground`, `chessops`, `three`, `@react-three/*`, `aceternity`, `magicui`, `gsap`, `lenis` |
    | `B13` | imports of `lucide-react` |
    | `B5a` | a className that contains both `uppercase` and `tracking-` |
    | `B5b` | `→` in `.tsx` text or strings |
    | `B5c` | `#0B0B0B`, `#111111`, `#111\b` |
    | `B6` | `Inter`, `Geist`, `Space_Grotesk`, `Instrument_Serif` imported from `next/font` |
    | `B7` | `bg-gradient-`, `bg-linear-`, `bg-radial-`, `bg-clip-text`, `backdrop-blur` |
    | `B15` | `lorem` (case-insensitive) |
    | `B18` | `animate-spin` |
    | `TOK1` | raw hex outside `tokens.css`, `board/**` and `annotations/**` |
    | `TOK2` | arbitrary Tailwind length values `-[\d+px]` outside `board/**` and `annotations/**` |

- [ ] **Step 1:** Write the failing test `ban-lint.test.ts`, with one positive and one negative sample per rule. For example, `className="uppercase tracking-wide"` is flagged `B5a` and `className="uppercase"` is not. `#D3312B` in `tokens.css` is not flagged, and in `button.tsx` it is flagged `TOK1`.
- [ ] **Step 2:** Run `pnpm vitest run scripts/test/ban-lint.test.ts`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the test. Expected: PASS.
- [ ] **Step 5: Review Impeccable.**
  1. Run `npm view impeccable version` and `npm pack impeccable@<version>` into a new scratch directory. Don't install it or run it there.
  2. Read `package.json` (lifecycle scripts, `bin`, dependencies) and the `detect` entry point: any network calls, file writes outside the target, or telemetry.
  3. Record the findings, the exact version and the tarball SHA-512 in ADR-WEB-9.
  4. If it is clean, put that version into the root `design:check` script. The script line is owned by W0, so this is a one-line request to the W0 owner, or folded into Task 27.
  5. If it isn't clean, `design:check` runs ban-lint only and the ADR says why.

  Installing the Impeccable Claude Code plugin is the controller's or owner's call. Record it as a recommendation; don't change user settings.
- [ ] **Step 6: Review Vercel `web-design-guidelines`.** Read the SKILL.md from `vercel-labs/agent-skills` at a pinned commit. Check its licence, and whether it fetches rules at runtime from a URL (if so, pin that URL to a commit). If it is acceptable, vendor it to `.claude/skills/web-design-guidelines/SKILL.md` with a header naming the source commit and licence. Record gsap-skills as deferred to M5.
- [ ] **Step 7:** Run `pnpm design:check`. Expected: `0 findings` on the current tree.
- [ ] **Step 8:** Commit `feat(qa): ban-list lint and reviewed third-party design tooling`.

### Task 26: Project skills `design-system` and `chess-ui`

**Files:**
- Create: `.claude/skills/design-system/{SKILL.md,tokens.md,components.md,motion.md}`, `.claude/skills/chess-ui/SKILL.md`

**Interfaces:**
- Consumes: the settled tokens (Task 5), the primitives and overlays (Tasks 7 and 8), the board and annotations (Tasks 11 and 12), the charts (Tasks 13–16), the decision files and the critique logs.

- [ ] **Step 1:** Load `superpowers:writing-skills`. Write `design-system/SKILL.md`, with the frontmatter description `"Use whenever creating or changing any UI, page, component, style, animation, or chart in this repo."` The body covers:
  - The non-negotiables: tokens only, no new font, colour, radius, shadow or easing without proposing it first, and the ban list by reference to `packages/ui/DESIGN.md`.
  - The workflow: plan, then lab for a new surface, then build, then CL.
  - The definition of done: shots at 390 and 1440 in both themes, `pnpm design:check` clean, and a critique `pass`.

  `tokens.md` explains what each token is for; the values stay in `tokens.css` only. `components.md` covers which primitive to use for what, their variants and required states (loading via `ProgressPlaceholder`, empty, error, long content). `motion.md` covers what animates, the durations and springs, what never animates, and reduced motion. A "Tried and rejected" section is collected from the `qa/log` files.
- [ ] **Step 2:** Write `chess-ui/SKILL.md`, with the description `"Use when building or changing anything that shows a chess board, moves, notation, evaluations, or chess charts."` It covers:
  - The board treatment chosen in Task 10 and its piece-set licence.
  - Annotation tones: `pen` for the played mistake, `good` for the better move, `pencil` for plans and threats.
  - The glyph set and the halo rule.
  - Notation formatting via `formatNotation` (U+2212, `31…` for Black).
  - Eval bar and eval graph conventions: White's point of view, ticks only for mistakes and above.
  - Keyboard behaviour of the board.
  - Never chessground or chessops.
- [ ] **Step 3: Validate the skills.**
  1. A fresh subagent with the skills loaded gets the task "add a 'Copy FEN' button under the board in the styleguide board section".
  2. Check that its diff uses `Button`, the tokens and `Icon`, passes `pnpm design:check`, and produces a toast that reads "FEN copied".
  3. Discard the diff.
  4. Fix the skill text if any check failed, and repeat once.
- [ ] **Step 4:** Commit `docs(skills): design-system and chess-ui project skills`.

---

## I: Integration

### Task 27: Integration: shell wiring, CI, CLAUDE.md, ADR index

**Files:**
- Modify:
  - `apps/web/src/app/providers.tsx` (adds `QueryProvider` and `Toaster`)
  - `apps/web/src/app/(app)/layout.tsx` (mounts `RealtimeProvider`, and `ActivityIndicator` in `data-slot="activity"`)
  - `apps/web/src/app/(app)/shell/nav-command-menu.tsx` (items Accounts, Styleguide in dev, and Sign out)
  - `apps/web/src/app/(app)/app/accounts/accounts-view.tsx` (uses `queryKeys.accounts()`)
  - `CLAUDE.md`, `docs/decisions/README.md`, `.github/workflows/ci.yml`, root `package.json` (the `design:check` version from Task 25)
- Test: `apps/web/e2e/realtime.spec.ts`

**Interfaces:**
- Consumes: everything above.

- [ ] **Step 1: Write the failing e2e test** `realtime.spec.ts`:
  1. Sign in as a user with a linked fake Lichess account.
  2. Insert and publish `import.progress { imported: 120, total: 400 }` for that account from the test (through `docker compose exec`, or a test-only helper that uses the M1 `publishEvent`).
  3. The header shows "Importing 120 / 400 games" within 2 s.
  4. Block `/api/events` SSE with `page.route` (abort), then reload, publish another event, and see the indicator update within 7 s via polling.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3: Wire it up.**
  - Add a CLAUDE.md "UI" section of at most 12 lines:
    - Load `design-system` (and `chess-ui` for board or graph work) before any UI change.
    - Tokens only.
    - Run `pnpm shots` and `pnpm design:check` and a fresh-subagent critique before claiming a UI change is done.
    - How to start the web stack and Mailpit (`localhost:3100` and `localhost:58025`).
    - How to run e2e and visual tests.
  - Index the ADR-WEB files in the README (the controller assigns their numbers).
  - CI runs the web and ui unit tests through the existing turbo steps, adds a `chess_web_test` database creation step, and runs `pnpm tsx scripts/ban-lint.ts`. Playwright e2e and visual tests stay local in M4; CI e2e is M8 on the roadmap.
- [ ] **Step 4:** Run `pnpm e2e realtime`. Expected: PASS.
- [ ] **Step 5:** Run the full check: `pnpm lint && pnpm typecheck && pnpm test`, then `docker compose run --rm web pnpm --filter @ci/web test`, then `pnpm e2e`. Expected: all pass.
- [ ] **Step 6:** Commit `chore(web): wire realtime and shell, ci checks, claude.md ui rules`.

### Task 28: Final critique sweep, quality checks and visual regression baselines

**Files:**
- Create: `apps/web/e2e/{visual,quality,cookies}.spec.ts`, `apps/web/e2e/__screenshots__/**`, `packages/ui/qa/log/final.md`

**Interfaces:**
- Produces:
  - `quality.spec.ts`, which loops over `/styleguide`, `/sign-in`, `/sign-in/check-email`, `/welcome/age`, `/blocked`, `/app/accounts` and `/lab`, with the authenticated routes using a stored state:
    - `no horizontal scroll at 360px`: `document.documentElement.scrollWidth <= 360`.
    - axe (`@axe-core/playwright`, tags `wcag2a`, `wcag2aa`) finds 0 `serious` or `critical` violations.
    - Tabbing from the top reaches the main action with a visible focus outline (computed `outline-style` isn't `none`).
  - `cookies.spec.ts`: after a full sign-in and linking flow, every cookie name matches `/^ci[._]/` and no cookie is set for a third-party domain.
  - `visual.spec.ts`: `toHaveScreenshot` for each `[data-section]` of `/styleguide`, plus `/sign-in` and `/blocked`, in the four visual projects. Settings: `animations: "disabled"` and `maxDiffPixelRatio: 0.001`.

- [ ] **Step 1:** Write `quality.spec.ts` and `cookies.spec.ts`, and run them. Fix any failures in the owning lane's files (send them back to the owning lane if it is still active).
- [ ] **Step 2:** Run CL one last time across `/styleguide`, `/sign-in`, `/app/accounts` and the chosen lab variants, with a fresh critic told to look for inconsistency *between* components. Log it in `final.md`.
- [ ] **Step 3:** Generate the baselines in the Linux container for determinism: `docker compose --profile qa run --rm qa sh -c "cd apps/web && E2E_BASE_URL=http://web:3100 npx playwright test --project='visual-*' --update-snapshots"`. Then rerun without `--update-snapshots`. Expected: PASS, with zero diffs.
- [ ] **Step 4:** Commit `test(web): quality checks, cookie allowlist and visual regression baselines`.

---

## Self-review

1. **Spec coverage.**
   - §6 stack and Better Auth: Tasks 1 and 17–18.
   - §7 chess.com username, code check, the 12-hour caveat and OAuth behind a flag: Task 19.
   - §11:
     - direction and tokens: Tasks 4–5
     - hard rules as B1–B18: Task 4, enforced in Task 25
     - board and eval graph as the signature: Tasks 10–13
     - red-pen motion: Task 12
     - eval bar spring: Task 11
     - quality floor: Global Constraints, Task 28
     - UI stack: Tasks 7, 8, 13 and 14
     - process steps 1–8: Tasks 4, 5, 6, 10, 24, the CL procedure and Task 28
     - skills: Tasks 25 and 26
     - Playwright MCP: Task 24
     - third-party review: Task 25
   - §13a client side:
     - SSE with Last-Event-ID replay: Task 22
     - TanStack Query patching, one entry at a time: Task 23
     - designed progress placeholders: Task 16
     - the global activity indicator and the non-modal failure notice: Task 23
     - the 5-second polling fallback: Tasks 22–23
     - the required reconnect test: Task 22

     Suspense streaming is only wired in the shell here; the sections that stream are M5 product pages.
   - §14:
     - under-13 block and teen defaults: Task 20
     - no consent banner because cookies are essential only: Tasks 20 and 28
     - encrypted OAuth tokens: Tasks 17–18
   - §18:
     - no deploy: Global Constraints
     - Google credentials are optional until the owner provides them
     - brand in one file: Task 1
   - Gap noted on purpose: the job retry button on failure notices waits for a retry endpoint (M5/M7). Task 23 shows the failure without one.
2. **Step scan.** Every code step names its file, its signature and the values it pins. The two steps that depend on a library option whose name I couldn't confirm (Better Auth `generateId`/`encryptOAuthTokens`, and Next's development-condition option) carry a decided fallback and a test that pins the outcome. No step says "handle errors" without the error copy.
3. **Type consistency.**
   - `StreamEvent` (Task 2) is used in Tasks 22–23.
   - `Annotation` and `Square` (Tasks 11–12) are used by Board and the skills.
   - `AppUser` (Task 17) is used in Tasks 19–21.
   - `WeaknessState` matches §12b's six states.
   - `queryKeys.accounts()` is used in Tasks 21 and 27.
   - `ProgressPlaceholder`'s `verb` values match the copy tests.
   - `formatEval` and `formatNotation` match the notation in DESIGN.md.
4. **Review Focus.** All five lines have a named test in their owning task: Tasks 19, 22, 18, 17, and 13, 15 and 28.
5. **Proportion.** Signatures, tokens, routes, copy and assertions only. The longest code block is the notation test values. No component bodies are written out.
