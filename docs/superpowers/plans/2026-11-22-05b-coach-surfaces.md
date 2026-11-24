# M5b Coach Surfaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The private-beta player (ADR 0041) uses the AI coach in the real app: a coaching report that fills in section by section, a ranked weaknesses list with a detail page per weakness, seven skill-area pages, a progress page with KPI trends and weekly reviews, an "Ask your coach" chat that streams, and an overview whose "What to work on" slot shows the profile's lead finding.

**Architecture:** `apps/web` reads the R2 profile tables and the R3 coaching tables through new read-only server modules, `apps/web/src/server/{profile,coaching,coach}`. Every read goes through M5a's `readOnly` and `scopedAccountIds`. The only writes are R3's documented web hand-offs: a `report_runs` row with its `generate-report` job, and the coach-chat rows (`coach_conversations`, `coach_messages`) with their `coach-chat` job, both through the `jobs` table. The worker does all AI work. Report progress and chat chunks reach the browser over the existing `/api/events` stream (`user_events`, with the per-user filter). The web never talks to the sidecar. Pages are server components; client islands handle the stream, the composer, the board and the motion. New UI pieces live in `@ci/ui` (`coach`, `profile`) and are built with the `design-system` and `chess-ui` skills.

**Tech Stack:** Next 16.4.6 App Router, React 19.3.1, Drizzle 0.45.4 on postgres 4.1.1, `@ci/ui` (Board, AnnotationLayer, AreaScore, WeaknessTimeline, KpiStat, Sparkline, ProgressPlaceholder, primitives, overlays, motion tokens), @visx/visx 4.0.0, Motion 14.14.0, @tanstack/react-query 5.107.1, Vitest 5.1.0 with Testing Library and jsdom, and @playwright/test 1.65.1. **No new dependencies**, in TS or Rust.

**Spec:** `docs/superpowers/specs/2026-11-21-chess-improver-design.md`: §5 (rating-aware coaching: the report's tone comes from R3, the page shows the band), §8b (seven skill areas: their own page, KPIs with n and band, weaknesses, "12 endgames so far: need 20" honesty), §8c "Shown to the user" (basis lines), §9 (AI coaching: facts by reference, validator, versioned reports), §10.3 (report), §10.6 (progress), §10a.4 (before/after leads progress), §10a.9 ("Ask your coach"), §11 (visual and motion), §12a (KPIs, progression, "Making it honest", "Plotting it"), §12b UI (weakness timeline, onset, states), §13a "Non-blocking, progressive UI", §14 "AI disclosure". Roadmap row 5 (`docs/superpowers/plans/2026-11-21-roadmap.md`). Backend contracts: `docs/superpowers/plans/2026-11-21-R2-profile-engine.md`, `docs/superpowers/plans/2026-11-21-R3-ai-coaching.md`, ADRs 0059 (tables, jobs, events, web hand-off), 0060 (facts by reference, segments), 0061 (budgets and degradation), 0062 (sidecar), 0063 (batches) and 0064 (chat, §10–§20). Style and procedures: `docs/superpowers/plans/2026-11-21-05a-game-pages.md` and the M4 plan's "Shared procedures" (CL).

**Base:** `m5b/base`, branched from `main` at 6616019. Worktrees go in `.worktrees/m5b-<lane>` (never outside the project).

## Global Constraints

- **Honesty (owner, §12a):** every rate or mean shows its n ("38 of 902 chances", "Based on 214 analysed rapid games"). Unknown is never shown as 0: a null value renders as a stated gap ("No games this week", "Need 7 more games"), never `0`, `0%`, `—` alone or an empty bar. No `NaN`, `Infinity`, `undefined` or `null` is ever rendered. Labels never overclaim: a posterior is "estimated rate", a state change is "detected", correlated causes are "possible causes", and nothing says a rating change was caused by training. Intervals are shown exactly as stored (80%, `ci_low`/`ci_high`, `weakness_estimates.ci_low`/`ci_high`); the web never widens, narrows, pads or recomputes a stored interval. Intervals the web computes itself (weekly weakness rates) use M5a's `wilson(k, n, 0.8)`.
- **Difficulty is Maia-only (ADR-R2-13):** the web never estimates difficulty. Positions whose `move_features.difficulty_status = 'pending_maia'` are counted and stated ("Difficulty adjustment is waiting for Maia on 112 of 3,040 positions"); they are never shown as easy, hard or adjusted.
- **AI disclosure (§14):** every surface showing model text shows the stored `ai_disclosure` ("Written by AI", its note and date). Model text is only ever rendered from stored `segments` (ADR 0060); raw `text` with `{{…}}` tokens is never rendered.
- **Privacy:** a chat thread is visible, postable and deletable only by its `coach_conversations.user_id`. Another user linked to the same account gets not-found (404), never 403. User-owned events (`user_events.user_id` not null) reach only their user (`eventVisibleTo`, ADR 0064 §16, §18). Report, weekly-review and note rows are account data, readable by every user linked to the account (ADR 0064 §18).
- **Scope:** a user sees profile and coaching data only for accounts in `userAccountIds` (verified or not, ADR 0064 §19, M5a R1). A foreign or malformed id is not-found.
- **Writes:** no new migration. The web writes only `report_runs`, `jobs`, `coach_conversations`, `coach_messages`, and deletes `coach_conversations` (the trigger and cascades do the rest, ADR 0059 "Deletion"). Everything else is read in `readOnly`.
- **UI bar (owner, §11):** premium, not generic. Load the `design-system` skill for every UI change and the `chess-ui` skill for anything with a board, notation, evals or chess charts. `packages/ui/DESIGN.md` is the law: ban list B1–B18 (no card grid, no stat banners, no all-caps eyebrows, no `→` on buttons, no spinners, no fade-up on every section), tokens only, icons only from `@ci/ui/icons`, `pnpm design:check` reports `0 findings`. Colour carries chess meaning: `pen` red is the player's own mistake, `pencil` is the opponent or an idea, `good` is the better move, never series colours. Every encoding is redundant (shape or pattern as well as colour).
- **UI bar enforced:** every UI task ends with CL (M4 "Shared procedures"): `pnpm shots` at 390 and 1440, light and dark; then a **fresh, independent critic** subagent that gets only the PNGs, `DESIGN.md` and `packages/ui/design/references/REFERENCES.md`, never source code; fix every blocker and major; stop at verdict `pass` after at least 2 passes; escalate after the third. Each pass is logged in `packages/ui/qa/log/<surface>.md`.
- **Quality floor:** 360px with no page-level horizontal scroll, visible focus, WCAG AA, keyboard-operable everything (the board's keys from M5a), `prefers-reduced-motion` (choreography becomes instant state with a brief opacity cue). Motion uses only transform, opacity and SVG `stroke-dashoffset`, tokens from `@ci/ui/motion`, is interruptible and never blocks input.
- **Copy:** sentence case, British spelling ("analysing", "colour"), active verbs, the real minus U+2212, dates in UTC at day precision ("12 Nov 2026") in `<time dateTime>`. ISO weeks show as "Week of 9 Nov 2026". Errors say what happened and how to fix it. Fixed Rust copy (chat caps, fallbacks, notes) is mirrored byte for byte and pinned by a parity test.
- **Tests:** TDD. Write the failing test first, see it fail, then implement. Vitest in `<package>/test/**` (`apps/web/test/...`, `packages/ui/test/...`). DB tests run in `$WEB` against `chess_web_test`.
- **AI in tests:** no live AI call anywhere (unit, DB or e2e). The web never calls AI. E2e plays the worker's part with `simulateWorker` (Task 11), which writes exactly the rows and events the R3 jobs write. The one Rust test (Task 1) needs no AI.
- **Owner data:** the owner's games, profile, reports or chats never become committed fixtures, screenshots or logs. Tests use synthetic rows, the public fixture games in `@ci/ui/fixtures`, and R3's synthetic `backend/fixtures/ai/payload/*.json`. E2e seeds refuse a database that holds real accounts (M5a's guard).
- **ADRs:** the next free numbers are **0068–0074** (0067 is reserved elsewhere; check `docs/decisions` again before writing, and shift all of them up together if 0068 is taken). They are assigned per task below. Only Task 11 edits `docs/decisions/README.md`.
- **Commits:** Conventional Commits, each ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_016XcpEwGx7ugY9Hcujn2JhE`.
- **Commands:**
  - `$WEB` is `COMPOSE_FILE=docker-compose.yml docker compose -p ci-m5b-<lane> run --rm web`; tear down with `COMPOSE_FILE=docker-compose.yml docker compose -p ci-m5b-<lane> down -v` (never `down -v` on the default project).
  - `$RUST` is `COMPOSE_FILE=docker-compose.yml docker compose -p ci-m5b-d run --rm -e LANE=d rust-dev` (CLAUDE.md).
  - UI tests run on the host: `pnpm --filter @ci/ui exec vitest run <file>`.
  - E2e and authenticated shots run only against a throwaway stack (`-p ci-m5b-e2e`, M5a Global Constraints), never the default project, which holds the owner's games.

## Rulings (the owner delegated all choices; each is recorded in the ADR named)

- **R1 (ADR 0068):** The coach scope is one (account, time class). URL params `account` and `tc` choose it; the default is the user's (account, time class) with the most `game_metrics` rows (ties: lichess before chesscom, then time class order bullet, blitz, rapid, classical, daily). A foreign, malformed or empty choice falls back to the default. Every coach page shows a scope switch when the user has more than one option.
- **R2 (ADR 0068):** Weakness exemplars (positions on the board) come from the newest `report_runs.input` that is not null for the scope, the same snapshot the coach chat reads (ADR 0064 §9). The page says which report they come from ("Examples from your report of 12 Nov 2026"). Without one: "Example positions appear once your first report is built."
- **R3 (ADR 0068):** A weakness's headline number is the stored posterior `mean` with its stored `ci_low`–`ci_high`, labelled "estimated rate", next to the raw counts `k_unweighted` of `n_unweighted` chances. It is "sufficient" when `n_unweighted ≥ minEvidence(key)` (the Rust `min_evidence`); below that it reads "Early read: N more chances needed" and the state chip says "early". The posterior is never relabelled as the raw rate.
- **R4 (ADR 0069):** "Build my report" and "Rebuild report" insert a `report_runs` row and its `generate-report` job in **one** transaction (stronger than Rust's two-step; the reconciler still covers a lost job). The trigger is `onboarding` when the scope has no `reports` row, otherwise `manual`. `plan` is `AI_PLAN = "free"`, `prompt_version` is `"report@1"`, `mode` `sync`. A second request while a run is live (`queued`, `running`, `awaiting_retry`, `awaiting_batch`) returns that run. Requests are serialised with `pg_advisory_xact_lock(30030, hashtext(account_id || ':' || time_class))` (class 30030 is unused; R3 uses 30001–30004 and 30020). A `manual` request inside the free interval (30 days since the newest other report, `FREE_REPORT_INTERVAL_DAYS`) is refused on the web with its date, so the player never waits for a job that will fail `plan_interval`.
- **R5 (ADR 0070):** Chat streams over the existing event stream: the web inserts the rows and the `coach-chat` job, and the worker's `chat.chunk`, `chat.message.ready` and `chat.message.failed` events arrive through `/api/events`. There is no web-to-sidecar route and the sidecar token is never in the web's env. On reload mid-answer, `getThread` returns the chunks already stored in `user_events` for that message (owner-filtered), so nothing streamed is lost.
- **R6 (ADR 0070):** The web shows the chat allowance (counted exactly like the job: assistant messages in the scope's conversations that are `streaming`, or `done` with a non-null `ai_disclosure`, per UTC window) and disables the composer when it is exhausted, with the reset copy. It never refuses a post for quota: the job is the authority and its denial arrives as `chat.message.failed`. Tool use is shown only as the worker exposes it: the interim engine line, `eng` segments marked as a server engine check, the message's `interactive_evals` rows, and the fixed dropped-statement and engine-limit notes. Nothing else is invented.
- **R7 (ADR 0073):** M5b builds directly in M5a's settled layout language (overview sheet, review page) with no LP lab variants. The bar is kept by CL with at least 2 passes per surface.
- **R8 (ADR 0072):** `AreaScore`'s `band.next` becomes optional. A percentile score has the band median at 50 by construction; the next band's position is not measured yet, so it is not drawn and the text says "next band not measured yet". Mindset has no score by design and says so ("Mindset is coaching advice, not a score").

## Review Focus

1. **Another user of the same linked account** opens, posts to or deletes a thread, or listens to the stream while its owner chats. They get not-found and receive no `chat.*` event. Tests: Task 5 `thread of another user is not found`, `post to another user's thread is not found`, `delete of another user's thread deletes nothing`, and `streamed chunks are the owner's only`.
2. **A report caught mid-flight or degraded:** sections ready out of order, a section dropped by the validator, a run `awaiting_retry` after an outage, `failed` with `plan_interval`, `budget`, `no_input` or `reconcile_cap`, a computed-only report because `ai_disabled_reason = 'no_api_key'`. Each shows its own honest state, the computed part stays visible, and no section is shown as written that is not `ready`. Tests: Task 2 `reportState` table, Task 6 `renders every report state without NaN`.
3. **Thin or absent evidence:** a weakness with 3 chances, an area with a null score, Mindset, a KPI week with `n = 0` or `value = null`, a scope with no `game_metrics`, positions `pending_maia`. No zero stands in for unknown, and "need N more" appears. Tests: Task 1 `insufficient weakness is labelled early`, `null kpi value stays null`, Task 4 `null points are gaps, not zeros`, Task 8 `mindset says it has no score`.
4. **Double submits:** two "Rebuild report" clicks at once, a second question while an answer streams, a refresh that re-posts. One run, one pending answer. Tests: Task 2 `concurrent requests create one run`, Task 5 `second post while answering is busy`.
5. **Reload or tamper mid-answer:** a reload while the coach is streaming, a `?anchor=` that is malformed, foreign or for another account, `?account=<foreign uuid>&tc=hyper`, `/app/weaknesses/<garbage>`, `/app/areas/kingside`. The answer resumes from stored chunks; bad anchors are dropped; bad params fall back to the default scope; unknown keys are not-found. Tests: Task 5 `thread replays stored chunks`, `parseAnchor rejects bad input`, Task 1 `resolveCoachScope falls back`, Task 7 `unknown series key is not found`, Task 8 `unknown area is not found`.

---

## Lanes and order

| Lane | Tasks | Owns (disjoint) |
|---|---|---|
| D Data | 1, 2 | `apps/web/src/server/{profile,coaching,jobs}/**`, `apps/web/src/app/api/coach/report/**`, `apps/web/test/{profile,coaching}/**`, `apps/web/test/helpers/{profile,coaching}.ts`, `backend/crates/profile-core/tests/catalogue_snapshot.rs`, `backend/fixtures/profile/**` |
| U UI kit | 3, 4 | `packages/ui/src/{coach,profile}/**`, `packages/ui/src/area-score/area-score.tsx`, `packages/ui/src/fixtures/types.ts` (the `band.next` line only), `packages/ui/test/{coach,profile,area-score}*.test.tsx`, `apps/web/src/app/styleguide/sections/{coach,profile}.tsx`, `apps/web/src/app/styleguide/page.tsx` (two section entries) |
| C Coach chat | 5, 9 | `apps/web/src/server/coach/**`, `apps/web/src/app/api/coach/threads/**`, `apps/web/src/lib/realtime/{coach-events.ts,reducers.ts}`, `apps/web/src/lib/query-keys.ts`, `apps/web/src/components/{live-refresh-on.tsx,coach-scope-switch.tsx}`, `apps/web/src/lib/segment-href.ts`, `apps/web/src/app/(app)/app/coach/**`, `apps/web/test/{coach,realtime}/**`, `apps/web/test/helpers/coach.ts` |
| P Pages | 6, 7, 8, 10 | `apps/web/src/app/(app)/app/{report,weaknesses,areas,progress}/**`, `apps/web/src/app/(app)/app/_overview/headline.tsx`, `apps/web/src/app/(app)/app/page.tsx`, `apps/web/test/pages/**` (one file per task) |
| I Integration | 11 | `apps/web/src/app/(app)/_shell/{nav.ts,nav-command-menu.tsx}`, `apps/web/e2e/**`, `docs/decisions/README.md`, `packages/ui/qa/log/m5b-final.md` |

```
T1 ─────────────┬──────────────▶ T7, T8, T10 ─┐
T2 (step 1: jobs/enqueue.ts) ─▶ T5 ─▶ T9 ─────┤
T2 ─────────────┼──────────────▶ T6 ──────────┼─▶ T11
T3 ─────────────┼─▶ T6, T8, T9, T10           │
T4 ─────────────┴─▶ T6, T7, T8, T10 ──────────┘
(T6, T7, T8 and T10 also need T5's coachHref, segmentHref, CoachScopeSwitch and LiveRefreshOn)
```

T1, T2, T3, T4 and T5 start at once (T5 rebases onto T2's first commit before its Step 3). T6, T7, T8 and T10 touch disjoint folders and run in parallel once their inputs land. T9 runs beside them. T11 is last.

---

### Task 1: Profile read model (weaknesses, areas, KPIs, difficulty, lead finding)

**Files:**
- Create: `backend/crates/profile-core/tests/catalogue_snapshot.rs`, `backend/fixtures/profile/catalogue.json`
- Create: `apps/web/src/server/profile/{types.ts,catalogue.ts,scope.ts,weaknesses.ts,areas.ts,kpis.ts,difficulty.ts,lead.ts,index.ts}`, `apps/web/test/helpers/profile.ts`, `apps/web/test/profile/{catalogue.test.ts,scope.test.ts,weaknesses.test.ts,areas.test.ts,kpis.test.ts,lead.test.ts}`, `docs/decisions/0068-coach-read-models.md`

**Interfaces:**
- Consumes: `readOnly`, `scopedAccountIds` (`server/games`), `wilson` (`server/stats`), `userAccountIds`; tables `weakness_estimates`, `weakness_state_history`, `weakness_series_point`, `kpi_snapshot`, `game_metrics`, `game_openings`, `opening_lines`, `move_features`, `report_runs` (`input` only), `linked_accounts` from `@ci/db`; `TimelineSeries`, `WeaknessState`, `Area` types from `@ci/ui/timeline` and `@ci/ui/area-score`. Rust: `profile_core::kpi::catalogue::kpi_catalogue()`, `profile_core::constants::{weakness_label, weakness_area, min_evidence, RANKABLE}`, `WeaknessKey::ALL`, `MotifTag::ALL` (`contract_tag()`).
- Produces:
  - `catalogue.json` (written by the Rust test): `{ weaknesses: { key, contractId, variant, label, area, minEvidence, rankable }[], motifs: { tag, contractTag }[], kpis: { metric, area, kind: "mean"|"rate"|"last", unit: "rate"|"per_game"|"pct"|"seconds"|"elo"|null, higherIsBetter: boolean|null, minN, label }[] }`, in each Rust list's order.
  - `catalogue.ts`: `WEAKNESSES`, `KPIS`, `weaknessDef(key: string)`, `kpiDef(metric: string)`, `seriesArea(seriesKey: string): Area` (`W*` by `area`, `motif.*` tactics, `opening:*` openings, `endgame:*` endgame), `seriesLabel(seriesKey: string, openingName?: string | null): string` (`W*` the Rust label; `motif.missed:<tag>` the KPI label of `motif_miss_rate:<tag>`; `motif.allowed:<tag>` "Allowed " + the tag's words, e.g. "Allowed forks"; `opening:<color>:<slug>` the opening name, else the slug in words, with "(as White)"/"(as Black)"; `endgame:<type>` "Rook endgames", etc.), `contractId(seriesKey): string | null` (the `RankedWeakness.id`, `W3.shallow` → `W3`, non-`W` → null).
  - `types.ts`:

    ```ts
    export type CoachScope = { accountId: string; timeClass: TimeClass; platform: "lichess" | "chesscom"; username: string; options: { accountId: string; platform: "lichess" | "chesscom"; username: string; timeClass: TimeClass; analysedGames: number }[] };
    export type StoredInterval = { low: number; high: number; level: 0.8 };
    export type WeaknessSummary = { seriesKey: string; contractId: string | null; label: string; area: Area; state: WeaknessState; rate: { mean: number } & StoredInterval; evidence: { events: number; chances: number; minEvidence: number | null; sufficient: boolean; needMore: number | null }; band: { median: number | null; p40: number | null }; priority: number; confidence: number; pImp: number | null; updatedAt: Date; onset: { at: Date; low: Date | null; high: Date | null; gameId: string | null } | null };
    export type Exemplar = { gameId: string; ply: number; playedAt: string; fen: string; played: { san: string; uci: string }; best: { san: string; uci: string } | null; bestPv: string[]; refutationPv: string[]; tags: string[]; winBefore: number; winAfter: number };
    export type WeaknessDetail = WeaknessSummary & { history: { kind: "transition" | "onset"; fromState: WeaknessState | null; toState: WeaknessState; at: Date | null; onsetAt: Date | null; onsetLow: Date | null; onsetHigh: Date | null; pChange: number | null; rateBefore: number | null; rateAfter: number | null; causeHints: unknown[] }[]; timeline: TimelineSeries; weekly: { week: string; events: number; chances: number; rate: ({ value: number } & StoredInterval) | null }[]; exemplars: { fromReportAt: Date; items: Exemplar[] } | null };
    export type KpiPoint = { week: string; value: number | null; low: number | null; high: number | null; n: number };
    export type KpiCurrent = { metric: string; label: string; unit: string | null; higherIsBetter: boolean | null; value: number | null; low: number | null; high: number | null; n: number; minN: number; sufficient: boolean; needMore: number | null; computedAt: Date };
    export type KpiTrend = { metric: string; label: string; unit: string | null; current: KpiCurrent | null; points: KpiPoint[] };
    export type AreaView = { area: Area; score: { value: number | null; n: number; computedAt: Date } | null; kpis: KpiCurrent[]; weaknesses: WeaknessSummary[]; timeline: TimelineSeries[] };
    export type DifficultyStatus = { positions: number; pendingMaia: number; ok: number; notApplicable: number };
    export type LeadFinding = { kind: "weakness"; scope: CoachScope; weakness: WeaknessSummary } | { kind: "none"; scope: CoachScope; analysedGames: number } | { kind: "building" };
    ```
  - `resolveCoachScope(db: Db, userId: string, params: Record<string, string | string[] | undefined>): Promise<CoachScope | null>` (R1; null when no option has a `game_metrics` row).
  - `listWeaknesses(db, userId, scope): Promise<WeaknessSummary[]>`: every `weakness_estimates` row of the scope, by `priority` desc, then label. `onset` is the newest `onset` row of `weakness_state_history` for the series.
  - `getWeakness(db, userId, scope, seriesKey): Promise<WeaknessDetail | null>`: null for an unknown or foreign key. `timeline` turns the transition rows into `TimelineSeries.segments` (each `to` is the next row's `at`, the last is `null`). `weekly` groups `weakness_series_point` by ISO week (UTC) over the last 26 weeks with a point, with `rate` null when `chances = 0`. `exemplars` follow R2 (match `contractId` and the variant).
  - `getArea(db, userId, scope, area: Area): Promise<AreaView>`; `listAreaScores(db, userId, scope): Promise<{ area: Area; value: number | null; n: number }[]>` (the seven areas in `areaLabels` order, from the newest `kpi_snapshot` row of `area:<id>` in window `last50`).
  - `kpiTrends(db, userId, scope, metrics?: string[]): Promise<KpiTrend[]>`: per metric, the newest row per `window_key` (`DISTINCT ON (metric, window_key) ... ORDER BY computed_at DESC`), weeks oldest first; `current` is the newest `last50` row.
  - `difficultyStatus(db, userId, scope): Promise<DifficultyStatus>` over the scope's games' `move_features.difficulty_status`.
  - `leadFinding(db, userId): Promise<LeadFinding>` over the default scope: the top `priority > 0` weakness whose state is `active`, `emerging` or `relapsed` and that is `sufficient`.
  - Test helper `seedProfile(db, accountId, opts: { timeClass: TimeClass; games?: number; estimates?: Partial<WeaknessEstimateRow>[]; history?: Partial<StateHistoryRow>[]; points?: Partial<SeriesPointRow>[]; kpis?: Partial<KpiSnapshotRow>[]; input?: unknown; difficulty?: ("ok" | "pending_maia" | "not_applicable")[] }): Promise<void>`, built on M5a's `seedAccount` and `seedSyntheticGame`.

- [ ] **Step 1: Write the failing tests.**
  - Rust `catalogue_snapshot.rs`, `catalogue_json_matches_the_fixture`: builds the JSON above from the public Rust items and compares it with `backend/fixtures/profile/catalogue.json`. With `UPDATE_SNAPSHOTS=1` it writes the file instead.
  - `catalogue.test.ts`:
    - `TS catalogue equals the Rust snapshot`: `WEAKNESSES` and `KPIS` deep-equal the fixture file read from `../../backend/fixtures/profile/catalogue.json`.
    - `seriesLabel("W1")` is `"Hung material"`; `seriesLabel("motif.allowed:fork")` is `"Allowed forks"`; `seriesLabel("opening:black:caro-kann-defence", "Caro-Kann Defence")` is `"Caro-Kann Defence (as Black)"`; `seriesLabel("endgame:rook")` is `"Rook endgames"`.
    - `seriesArea("W9b")` is `"time"`, `seriesArea("motif.missed:pin")` is `"tactics"`, and `contractId("W3.shallow")` is `"W3"`.
  - `scope.test.ts` (real Postgres):
    - `defaults to the option with most analysed games`: 30 rapid and 12 blitz `game_metrics` rows give rapid.
    - `resolveCoachScope falls back`: `{ account: "<foreign uuid>", tc: "hyper" }` and `{ account: "x'; drop" }` give the default.
    - `honours a valid choice`: `{ account: <own>, tc: "blitz" }` gives blitz.
    - `no analysed games gives null`.
  - `weaknesses.test.ts`:
    - `lists by priority with stored intervals`: three rows give priority order, and `rate.low`/`rate.high` equal the seeded `ci_low`/`ci_high` exactly.
    - `insufficient weakness is labelled early`: W1 with `n_unweighted = 120` (min 300) gives `sufficient: false, needMore: 180`.
    - `foreign series is not found`: user B gets null for user A's `W1`.
    - `unknown series key is not found`: `"W99"` gives null.
    - `timeline segments chain the transitions`: rows `not_an_issue → emerging → active` give three segments whose `to` dates chain, the last `null`.
    - `weekly rates use wilson and keep empty weeks null`: a week with 3 events of 60 gives the M5a `wilson(3, 60)` interval; a week with 0 chances gives `rate: null`.
    - `exemplars come from the newest report input`: two runs with inputs give the newer one's exemplars and its date; no run gives `exemplars: null`.
  - `areas.test.ts`: `null area score stays null` (a seeded `area:endgame` row with `value: null, n: 12` gives `{ value: null, n: 12 }`, not 0); `area weaknesses are filtered by area`; `listAreaScores returns all seven in order`, with missing areas as `{ value: null, n: 0 }`.
  - `kpis.test.ts`: `null kpi value stays null`; `newest snapshot per week wins` (two rows for `2026-W45`, the later `computed_at` wins); `weeks are oldest first`; `current is the newest last50 row`.
  - `lead.test.ts`: `lead is the top sufficient active weakness`; `resolved or insufficient weaknesses are not a lead` (gives `{ kind: "none", analysedGames }`); `no scope gives building`.
  - `difficulty.test.ts` is folded into `areas.test.ts`: `counts pending_maia positions` (2 `ok`, 3 `pending_maia`, 1 `not_applicable` give those counts and `positions: 6`).
- [ ] **Step 2:** Run `$RUST cargo test --locked -p profile-core --test catalogue_snapshot` (expected: FAIL, the fixture is missing) and `$WEB pnpm --filter @ci/web exec vitest run test/profile` (expected: FAIL, modules missing).
- [ ] **Step 3:** Write the Rust test, generate the fixture with `UPDATE_SNAPSHOTS=1`, and commit both (the coordinator rule on `Cargo.toml` is untouched: `serde_json` is already a dependency). Then implement the TS modules with the signatures above. ADR 0068 records R1–R3, the catalogue parity, the label rules, the area mapping of non-`W` series and the read-only boundary.
- [ ] **Step 4:** Run both commands. Expected: PASS. Then run `pnpm --filter @ci/web lint && pnpm --filter @ci/web typecheck`.
- [ ] **Step 5:** Commit `feat(web): profile read model for weaknesses, areas and KPI trends`.

**Reviewer gate:** a fresh reviewer mutation-checks the scope (dropping the account filter must fail a test) and the sufficiency rule, and checks that no code path turns a null into 0.

### Task 2: Coaching read model, report requests and the job enqueue

**Files:**
- Create: `apps/web/src/server/jobs/{enqueue.ts,index.ts}`, `apps/web/src/server/coaching/types.ts`, `apps/web/test/coaching/enqueue.test.ts` (**first commit**, see Step 1)
- Create: `apps/web/src/server/coaching/{payload.ts,state.ts,report.ts,request.ts,weekly.ts,notes.ts,index.ts}`, `apps/web/src/app/api/coach/report/route.ts`, `apps/web/test/helpers/coaching.ts`, `apps/web/test/coaching/{payload.test.ts,state.test.ts,report.test.ts,request.test.ts,weekly.test.ts,notes.test.ts,route.test.ts}`, `docs/decisions/0069-report-requests-and-states.md`

**Interfaces:**
- Consumes: `readOnly`, `scopedAccountIds`, `rejectCrossSite`, `requireApiUser`, `getDb`; tables `reports`, `report_sections`, `report_runs`, `weekly_reviews`, `weakness_change_notes`, `jobs`; fixtures `backend/fixtures/ai/payload/report-payload.example.json` and `report-payload.partial.example.json`.
- Produces:
  - `enqueueJob(tx: Db, job: { kind: string; payload: unknown; dedupeKey: string; userKey: string; priority: number; maxAttempts: number }): Promise<number | null>`: the R1 `ENQUEUE_SQL` (`backend/crates/queue/src/queue.rs`), same columns, `run_at = now()`, `ON CONFLICT (kind, dedupe_key) WHERE dedupe_key IS NOT NULL AND state IN ('queued','running','paused') DO NOTHING`, and `pg_notify('jobs', kind)`. Null when a live job holds the key. Constants `JOB = { report: { kind: "generate-report", priority: 50, maxAttempts: 3, key: (runId) => "report:" + runId }, chat: { kind: "coach-chat", priority: 10, maxAttempts: 2, key: (messageId) => "chat:" + messageId } }`.
  - `types.ts`: `CoachTarget = { accountId: string; timeClass: TimeClass }` (Task 1's `CoachScope` is a superset, so the two tasks run in parallel; the functions below take a `CoachTarget` named `scope`); `SectionKey = "overview" | "weaknesses" | "openings" | "climb_plan" | "areas"` (`SECTION_KEYS` in that order); `Segment` (the Rust `segments.rs` union: `text`, `move`, `best`, `pv`, `line`, `num`, `date`, `square`, `eng`, camelCase fields); `NarrativeItem = { text: string; segments: Segment[]; citations: { gameId: string; ply: number }[] }`; `AiDisclosure = { label: string; note: string; model: string; promptVersion: string; generatedAt: string }`; `ComputedReport` and `SectionContent` mirroring `report-payload.example.json` (`computed.{player,basis,evidenceWindow,areaScores,metrics,weaknesses,strengths,openings,focuses,newThisPeriod,fixedThisPeriod}` and `sections.<key>.{overview|weaknesses|openings|climbPlan|areas}`); `ReportPayload = { schemaVersion: string; reportId: string; version: number; status: "computed_only" | "partial" | "complete"; computed: ComputedReport; sections: Record<SectionKey, SectionContent | null>; dropped: { section: string; path: string; errors: { code: string; path: string }[] }[]; aiDisclosure: AiDisclosure | null; generatedAt: string }`.
  - `assembleReportPayload(report: ReportRow, sections: ReportSectionRow[]): ReportPayload`: the TS port of `assemble_report_payload` (`backend/crates/ai/src/generate/payload.rs`): a `dropped` section's content is null, `dropped` lists every section's items, `aiDisclosure` is null for `computed_only`.
  - `state.ts`:

    ```ts
    export type ReportFailure = "plan_interval" | "no_input" | "invalid_input" | "ai_unavailable" | "ai_auth" | "ai_error" | "budget" | "job_dead" | "reconcile_cap" | "unknown";
    export type ReportState =
      | { kind: "none" }
      | { kind: "building"; runId: string; since: Date; retryAt: Date | null; batch: boolean; ready: SectionKey[] }
      | { kind: "ready"; status: "complete" | "partial" | "computed_only"; aiOff: "no_api_key" | "budget" | null }
      | { kind: "failed"; reason: ReportFailure; at: Date; hasPrevious: boolean };
    export function reportState(run: ReportRunRow | null, report: ReportRow | null, ready: SectionKey[]): ReportState;
    ```
    `building` covers the four live statuses (`batch` when `awaiting_batch`, `retryAt` from `next_attempt_at` when `awaiting_retry`). A `failed` run whose report exists shows `failed` with `hasPrevious: true`, and the page keeps the previous report. `aiOff` is the run's `ai_disabled_reason`.
  - `getReport(db, userId, scope): Promise<{ state: ReportState; payload: ReportPayload | null; createdAt: Date | null; nextManualAt: Date | null }>`: the newest run and the newest report of the scope; `nextManualAt` is the newest report's `created_at` + 30 days when that is in the future.
  - `requestReport(db, userId, target: { accountId: string; timeClass: TimeClass }, now?: Date): Promise<{ kind: "queued"; runId: string } | { kind: "building"; runId: string } | { kind: "interval"; availableAt: Date } | { kind: "not_found" }>` (R4).
  - `POST /api/coach/report` with JSON `{ accountId, timeClass }`: `rejectCrossSite`, `requireApiUser`; 202 `{ runId }` for queued and building (`building: true` for the latter), 429 `{ error: "plan_interval", availableAt }`, 404 `{ error: "not_found" }`, 400 `{ error: "invalid_body" }`.
  - `listWeeklyReviews(db, userId, scope, limit = 12): Promise<{ id: string; weekStart: string; status: "pending" | "ready" | "dropped" | "failed"; content: WeeklyContent | null; droppedCount: number; aiDisclosure: AiDisclosure | null }[]>`, newest first. `WeeklyContent` mirrors the validated weekly output in `backend/crates/ai/src/output/weekly.rs` (`headline`, `process`, `outcomes` with their computed `label`, `exemplars`, `nextWeek`, each a `NarrativeItem`); read that file for the exact stored shape.
  - `changeNotes(db, userId, accountId: string, seriesKeys?: string[]): Promise<{ changeKey: string; seriesKey: string; onsetDate: string | null; status: "pending" | "ready" | "dropped" | "failed"; item: NarrativeItem | null; aiDisclosure: AiDisclosure | null }[]>`. `item` is set only when `ready`.
  - Test helper `seedReport(db, accountId, opts: { timeClass: TimeClass; payload?: "example" | "partial"; run?: Partial<ReportRunRow>; createdAt?: Date }): Promise<{ runId: string; reportId: string | null }>` writes `reports.computed` and `report_sections` from the fixture; `seedWeekly`, `seedNote`.

- [ ] **Step 1: Land the enqueue first.** Write `server/coaching/types.ts` (types only) and `enqueue.test.ts`: `enqueues with the R1 columns` (the row has the kind, payload, dedupe key, `user_key`, priority, `max_attempts` and state `queued`); `a live duplicate is skipped` (null, one row); `a finished duplicate is enqueued again` (after setting the first to `done`). Run `$WEB pnpm --filter @ci/web exec vitest run test/coaching/enqueue.test.ts` (FAIL), implement, run again (PASS), and commit `feat(web): enqueue jobs into the shared jobs table and coaching types`. Task 5 rebases onto this commit (it imports `enqueueJob`, `JOB`, `Segment`, `SectionKey` and `AiDisclosure`).
- [ ] **Step 2: Write the failing tests.**
  - `payload.test.ts`: `assembles the example payload` (seeded from `report-payload.example.json`, the result deep-equals the fixture except `generatedAt`, `reportId` and `version`); `assembles the partial payload` (`status: "partial"`, the dropped section is null, `dropped` equals the fixture's).
  - `state.test.ts`, one case per row:

    | run, report | expected |
    |---|---|
    | none, none | `none` |
    | `queued`, none | `building`, `ready: []` |
    | `running`, report with `overview` ready | `building`, `ready: ["overview"]` |
    | `awaiting_retry` with `next_attempt_at` | `building`, `retryAt` set |
    | `awaiting_batch` | `building`, `batch: true` |
    | `done`, report `complete` | `ready`, `complete`, `aiOff: null` |
    | `done` with `ai_disabled_reason = 'no_api_key'`, report `computed_only` | `ready`, `computed_only`, `aiOff: "no_api_key"` |
    | `failed` with `last_error = 'plan_interval'`, previous report | `failed`, `plan_interval`, `hasPrevious: true` |
    | `failed` with `last_error = 'reconcile_cap'`, no report | `failed`, `reconcile_cap`, `hasPrevious: false` |
    | `failed` with `last_error = 'something_new'` | `failed`, `unknown` |
  - `report.test.ts` (real Postgres): `foreign scope sees nothing`; `next manual date is 30 days after the newest report`.
  - `request.test.ts`: `first request is onboarding` (trigger `onboarding`, `is_first_report` true, one `generate-report` job with payload `{"reportRunId": <id>}` and key `report:<id>`); `later request is manual`; `concurrent requests create one run` (two `Promise.all` calls give one run, one `queued` and one `building` with the same `runId`); `manual request inside the free interval is refused` (newest report 10 days old gives `interval` with that date + 30 days, and no row); `run and job commit together` (a forced failure in `enqueueJob` leaves no run); `foreign account is not found`.
  - `route.test.ts`: cross-site POST gives 403; a bad body gives 400; signed-out gives 401; the happy path gives 202.
  - `weekly.test.ts`: `ready review carries its content and disclosure`; `pending review has no content`; `foreign scope sees nothing`.
  - `notes.test.ts`: `only ready notes carry an item`; `filters by series key`.
- [ ] **Step 3:** Run `$WEB pnpm --filter @ci/web exec vitest run test/coaching`. Expected: FAIL.
- [ ] **Step 4:** Implement. ADR 0069 records R4, the state table, the copy each state maps to (Task 6 renders it), the advisory-lock class 30030, and why the web inserts the run and the job in one transaction.
- [ ] **Step 5:** Run the same command (PASS), then lint and typecheck, and commit `feat(web): coaching read model and report requests`.

**Reviewer gate:** the reviewer diffs `enqueueJob`'s SQL against `ENQUEUE_SQL`, diffs `assembleReportPayload` against `payload.rs`, and mutation-checks the live-run guard and the interval rule.

### Task 3: Coach text, disclosure and state components in `@ci/ui`

**Files:**
- Create: `packages/ui/src/coach/{index.ts,segment-text.tsx,ai-disclosure.tsx,coach-state.tsx,chat-message.tsx,chat-composer.tsx,types.ts}`, `packages/ui/test/coach.test.tsx`, `apps/web/src/app/styleguide/sections/coach.tsx`, `docs/decisions/0071-coach-text-and-states.md`
- Modify: `apps/web/src/app/styleguide/page.tsx` (adds the "Coach" section)

**Interfaces:**
- Consumes: `@ci/ui` primitives, `ProgressPlaceholder`, `@ci/ui/motion`, `@ci/ui/icons`, DESIGN.md "Severity and marks" and notation rules.
- Produces (plain props; `types.ts` redeclares `Segment` structurally so `@ci/ui` does not import from the web):
  - `SegmentText({ segments, hrefFor, as? }: { segments: Segment[]; hrefFor?: (s: Segment) => string | null; as?: "p" | "span" })`: `text` as text; `move`, `best` and `pv` as notation (SAN in the notation face, `best` with the `good` mark, linked when `hrefFor` returns a URL); `num` shows `display`; `date` a `<time dateTime>` in day precision; `square` the square name; `line` the SAN line; `eng` the text with a `pencil` "engine check" marker and the accessible name "Engine check: …". An unknown `kind` renders nothing and never throws.
  - `AiDisclosure({ disclosure, compact? })`: the label, the note, and "Written on 12 Nov 2026"; compact puts the note in a tooltip.
  - `CoachState({ kind, title, body, action?, progress? })` with `kind: "building" | "writing" | "failed" | "budget" | "unavailable" | "computed_only" | "empty"`. `building` and `writing` render `ProgressPlaceholder` (no spinner, B18); the rest render a quiet note with an optional action button.
  - `ChatMessage({ role, status, segments, streamed, notes, engineChecks, failure, disclosure, hrefFor })`: `role: "user" | "assistant"`; `status: "pending" | "streaming" | "done" | "failed"`; `streamed: { seq: number; segments: Segment[]; interim: boolean }[]` shown in `seq` order while not done (an interim chunk in the `pencil` aside style); `notes: string[]` (fixed server copy); `engineChecks: { status: "pending" | "done" | "failed"; moves: string[] }[]` listed as "Engine check: 1. e4 e5 2. Nf3 (done)"; `failure: string | null` (copy). `pending` with nothing streamed shows "Your coach is reading your games" as a `writing` placeholder, never a spinner.
  - `ChatComposer({ maxChars = 2000, disabled, disabledReason, onSubmit, placeholder })`: a textarea with a live "1,950 characters left" count from 200 left, Enter sends, Shift+Enter breaks the line, empty or over-limit input cannot send, and `disabledReason` is shown in place of the send button.

- [ ] **Step 1: Write the failing tests** (`coach.test.tsx`):
  - `renders every segment kind`: one of each kind renders the text, SAN, display number and date, and no `{{`.
  - `move segments link through hrefFor`: `{ kind: "move", gameId: "g1", ply: 5, san: "d4" }` with `hrefFor` returning `/app/games/g1?ply=5` gives a link with that href and the name "d4".
  - `unknown segment kind renders nothing`: `{ kind: "future" }` does not throw.
  - `eng segment is marked as an engine check`.
  - `pending message shows a writing placeholder, not a spinner`: no `role="progressbar"` without a value, and the text "Your coach is reading your games".
  - `streamed chunks render in seq order` (chunks 2, 0, 1 render as 0, 1, 2).
  - `composer refuses empty and over-limit input`: 2001 characters disable send and say "1 character over".
  - `composer sends on Enter, breaks on Shift+Enter`.
  - `AiDisclosure shows label and day-precision date`.
  - Across all components, rendered HTML has no `NaN|undefined|Infinity|null`.
- [ ] **Step 2:** Run `pnpm --filter @ci/ui exec vitest run test/coach.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Load the `design-system` and `chess-ui` skills, then implement. The streamed paragraph settles in with the `pen` spring on mount (reduced motion: instant). ADR 0071 records the segment rendering, the link rule, unknown-kind tolerance and the placeholder copy.
- [ ] **Step 4:** Run the same command. Expected: PASS.
- [ ] **Step 5:** Run CL on `/styleguide` (the Coach section in every state). Log it in `packages/ui/qa/log/coach-kit.md`.
- [ ] **Step 6:** Commit `feat(ui): coach text, disclosure, state and chat components`.

**Reviewer gate:** the critic's `pass`, plus a code review of the tokens-only rule, the unknown-kind path and the keyboard path through the composer.

### Task 4: Profile components in `@ci/ui` (weakness row, KPI trends, exemplar board)

**Files:**
- Create: `packages/ui/src/profile/{index.ts,weakness-row.tsx,kpi-trend-grid.tsx,exemplar-board.tsx,text.ts}`, `packages/ui/test/profile.test.tsx`, `apps/web/src/app/styleguide/sections/profile.tsx`, `docs/decisions/0072-profile-encodings.md`
- Modify: `packages/ui/src/area-score/area-score.tsx`, `packages/ui/src/fixtures/types.ts` (`band.next` optional), `packages/ui/test/area-score.test.tsx`, `apps/web/src/app/styleguide/page.tsx` (adds the "Profile" section; merge order with Task 3 is trivial)

**Interfaces:**
- Consumes: `Board`, `AnnotationLayer` (`@ci/ui/board`, `@ci/ui/annotations`), `stateStyle` and `WeaknessState` (`@ci/ui/timeline`), `Sparkline`, visx, `@ci/ui/motion`.
- Produces:
  - `weaknessRateText(r: { mean: number; low: number; high: number }, e: { events: number; chances: number }): string`: "Estimated 4.2% of chances (80% range 3.1–5.6%), from 38 of 902", one decimal, U+2013 in the range.
  - `WeaknessRow({ label, state, rate, evidence, band, href, trend, onset })`: `evidence: { events; chances; sufficient; needMore: number | null }`; `band: { median: number | null }`; `trend: (number | null)[]` (weekly rates, null gaps); `onset: string | null` (ISO date). The state chip uses `stateStyle` (pattern and colour); an insufficient row says "Early read" and "N more chances needed" instead of the comparison with the band.
  - `KpiTrendGrid({ items, range, events, minN })`: `items: { metric: string; label: string; unit: string | null; points: { week: string; value: number | null; low: number | null; high: number | null; n: number }[] }[]`; `range: { from: string; to: string }` (one shared time axis); `events?: { date: string; label: string }[]` (report dates as vertical pencil rules). Each small multiple draws its stored band where `low`/`high` exist, a line through points with `n ≥ minN`, hollow dots for points below it, and **gaps** for null values. Each has an accessible summary ("Blunders per game: 1.4 in week of 9 Nov 2026, 12 games; …").
  - `ExemplarBoard({ fen, orientation, played, best, refutation, caption })`: the position before the move, the played move as a `pen` arrow, `best` as a `good` arrow, the first refutation move as a `pencil` arrow, and the caption in notation ("14. d4? Better was Nxe5. Win chance 52% to 36%."). It is keyboard-reachable and static (no move input).
  - `AreaScore` with `band.next?: number`: absent, no next-band tick is drawn and `areaScoreText` says "next band not measured yet".

- [ ] **Step 1: Write the failing tests** (`profile.test.tsx`, `area-score.test.tsx`):
  - `weaknessRateText formats mean, range and counts` (`{ mean: 0.0421, low: 0.031, high: 0.056 }`, `{ events: 38, chances: 902 }` gives the string above).
  - `insufficient row says early read`: `needMore: 180` gives "180 more chances needed" and no band comparison.
  - `null points are gaps, not zeros`: points `[1.2, null, 0.8]` give two line segments, and the summary says "no games" for the null week.
  - `points below minN are hollow`.
  - `stored band is drawn as given`: the band path's ends map exactly to `low` and `high`.
  - `exemplar board draws played, best and refutation arrows` (three arrows with the `pen`, `good` and `pencil` classes).
  - `area score without next band draws no next tick` and its text says "next band not measured yet".
  - No rendered `NaN|undefined|Infinity`.
- [ ] **Step 2:** Run `pnpm --filter @ci/ui exec vitest run test/profile.test.tsx test/area-score.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Load `design-system`, `chess-ui` and `dataviz`, then implement. ADR 0072 records the weakness-rate wording, the small-multiple encodings (bands, gaps, hollow points, shared axis), the exemplar arrow colours and R8.
- [ ] **Step 4:** Run the same command. Expected: PASS.
- [ ] **Step 5:** Run CL on `/styleguide` (the Profile section). Log it in `packages/ui/qa/log/profile-kit.md`.
- [ ] **Step 6:** Commit `feat(ui): weakness row, KPI trend grid and exemplar board`.

**Reviewer gate:** the critic's `pass`, plus a check that no colour is the only carrier of meaning and that the reduced-motion path is instant.

### Task 5: Coach chat server, routes and the realtime coach events

**Files:**
- Create: `apps/web/src/server/coach/{anchor.ts,copy.ts,threads.ts,post.ts,allowance.ts,index.ts}`, `apps/web/src/app/api/coach/threads/route.ts`, `apps/web/src/app/api/coach/threads/[threadId]/route.ts`, `apps/web/src/app/api/coach/threads/[threadId]/messages/route.ts`, `apps/web/src/lib/realtime/coach-events.ts`, `apps/web/src/components/{live-refresh-on.tsx,coach-scope-switch.tsx}`, `apps/web/src/lib/segment-href.ts`, `apps/web/test/helpers/coach.ts`, `apps/web/test/coach/{anchor.test.ts,copy-parity.test.ts,threads.test.ts,post.test.ts,allowance.test.ts,routes.test.ts}`, `apps/web/test/realtime/coach-events.test.ts`, `apps/web/test/coach/{segment-href.test.ts,scope-switch.test.tsx}`, `docs/decisions/0070-chat-web-hand-off.md`
- Modify: `apps/web/src/lib/realtime/reducers.ts` (the `default` branch calls `applyCoachEvent`), `apps/web/src/lib/query-keys.ts`

**Interfaces:**
- Consumes: Task 2's `enqueueJob` and `JOB.chat`; `userAccountIds`, `readOnly`, `rejectCrossSite`, `requireApiUser`; tables `coach_conversations`, `coach_messages`, `interactive_evals`, `user_events`, `account_links`; Rust sources for parity: `backend/crates/ai/src/chat/{threads.rs,limits.rs,summary.rs,anchor.rs}`, `backend/crates/ai/src/entitlements.rs`.
- Produces:
  - `ChatAnchor` (the Rust serde form): `{ kind: "report_section"; reportId: string; section: SectionKey } | { kind: "weakness"; weaknessId: string } | { kind: "position"; gameId: string; ply: number } | { kind: "move"; gameId: string; ply: number } | { kind: "opening_line"; lineKey: string } | { kind: "game"; gameId: string }`.
  - `parseAnchor(raw: unknown): ChatAnchor | null` (strict: unknown keys, wrong types, a non-uuid `reportId` or `ply` outside 1..1000 give null); `encodeAnchor(a): string` and `decodeAnchor(s: string): ChatAnchor | null` (base64url JSON); `coachHref(anchor: ChatAnchor | null, scope: { accountId: string; timeClass: TimeClass }): string` → `/app/coach/new?account=…&tc=…&anchor=…`; `fallbackTitle(anchor)`.
  - `copy.ts`: `CHAT_COPY` with `fallback`, `refusal`, `offTopic`, `noData`, `unavailable`, `enginePending`, `droppedNote(n)`, `engineLimitNote(window)`, `capText(reason)` for `weekly_cap | daily_cap | monthly_cap | conversation_full | too_long`, plus web-only `failureCopy(reason: string): string` for `budget` ("Your coach has reached this month's spending limit. It will be back next month."), `not_linked` ("This account is no longer linked to you, so the coach can't answer here."), `lost` and `unavailable` (the Rust unavailable text), and any other reason ("Your coach couldn't answer that one. Please try again.").
  - `createConversation(db, userId, { accountId, timeClass, anchor }): Promise<{ kind: "created"; id: string } | { kind: "not_linked" }>`; `listConversations(db, userId, accountId, limit = 50): Promise<{ id: string; title: string; anchor: ChatAnchor | null; lastMessageAt: Date }[]>`; `deleteConversation(db, userId, id): Promise<boolean>`.
  - `getThread(db, userId, id): Promise<Thread | null>` with `Thread = { id; accountId; timeClass; title; anchor; messages: { id; seq; role; status; anchor; segments: Segment[] | null; failureReason: string | null; aiDisclosure: AiDisclosure | null; createdAt: Date; streamed: { seq: number; segments: Segment[]; interim: boolean }[]; engineChecks: { status: "pending" | "done" | "failed"; moves: string[] }[] }[] }`. `streamed` is filled only for `pending` or `streaming` assistant messages, from `user_events` rows with `type = 'chat.chunk'`, `user_id` = the user and `payload->>'messageId'` = the message.
  - `postUserMessage(db, userId, threadId, question: string, anchor: ChatAnchor | null): Promise<{ kind: "posted"; userMessageId: string; assistantMessageId: string } | { kind: "not_found" } | { kind: "not_linked" } | { kind: "busy" } | { kind: "empty" } | { kind: "too_long" }>`: the port of `post_user_message` (blank → `empty`; over 2000 characters by code points → `too_long`; `FOR UPDATE` on the conversation; `busy` while an assistant message is `pending` or `streaming`; user row `done` at `seq = max + 1`, assistant row `pending` at the next `seq`; `last_message_at = now()`), with the `coach-chat` job (`{"messageId": …}`, key `chat:<id>`, `user_key` the account) enqueued **in the same transaction**.
  - `chatAllowance(db, accountId, now?: Date): Promise<{ quotas: { window: "day" | "week" | "month"; limit: number; used: number; resetsAt: Date }[]; exhausted: { reason: "weekly_cap" | "daily_cap" | "monthly_cap"; resetsAt: Date } | null }>` for `AI_PLAN = "free"` (3 a week, weeks from Monday 00:00 UTC), counting as R6.
  - Routes: `GET /api/coach/threads?account=<id>` (list) and `POST /api/coach/threads` `{ accountId, timeClass, anchor?, question? }` (create, and post the first question when given) → 201 `{ id, assistantMessageId? }`; `GET /api/coach/threads/[threadId]` → `Thread` or 404; `DELETE` → 204 or 404; `POST /api/coach/threads/[threadId]/messages` `{ question, anchor? }` → 202 `{ userMessageId, assistantMessageId }`, 404, 403 `not_linked`, 409 `busy`, 400 `empty` or `too_long`. All mutating routes go through `rejectCrossSite`.
  - `queryKeys` gains `report: (accountId) => ["report", accountId]`, `weekly: (accountId) => ["weekly", accountId]`, `notes: (accountId) => ["notes", accountId]`, `thread: (threadId) => ["thread", threadId]`, `chatStream: (messageId) => ["chat-stream", messageId]`.
  - `applyCoachEvent(qc: QueryClient, e: StreamEvent): boolean` (true when handled): `report.section.ready`, `report.ready` and `report.degraded` invalidate `report(accountId)` (a `report.degraded` also toasts "Your coach's notes are paused: <reason copy>. The computed report is still here."); `weekly_review.ready` invalidates `weekly`; `weakness.note.ready` invalidates `notes`; `chat.chunk` appends `{ seq, segments, interim }` to `chatStream(messageId)` (deduplicated by `seq`); `chat.message.ready` and `chat.message.failed` set `chatStream(messageId)` to `{ done: true, replace?: boolean, reason?: string }`. Malformed payloads are ignored.
  - `LiveRefreshOn({ keys }: { keys: QueryKey[] })`: M5a's `LiveRefresh` behaviour for any keys (a throttled `router.refresh()` at most every `REFRESH_THROTTLE_MS`, imported from `live-refresh.tsx`).
  - `CoachScopeSwitch({ scope, basePath })` (props structurally match Task 1's `CoachScope`, so Task 5 does not wait for Task 1): a `SegmentedControl` or `Select` of the scope's options ("Rapid on lichess (214 games)"), writing `account` and `tc` to the URL. Hidden with one option. Used by Tasks 6, 7, 8, 9 and 10.
  - `segmentHref(s: Segment): string | null` in `lib/segment-href.ts`: `move`, `best` and `pv` link to `/app/games/<gameId>?ply=<ply>` (the M5a review ply, which shows the position after that ply); `line`, `eng` and the rest link to nothing (the opening explorer is out of scope).
  - Test helper `seedThread(db, { userId, accountId, timeClass, messages?: { role; status; text?; segments?; failureReason? }[] }): Promise<{ threadId: string; messageIds: string[] }>` and `seedChunk(db, { userId, accountId, messageId, seq, segments, interim? })`.

- [ ] **Step 1: Write the failing tests.**
  - `anchor.test.ts`: round trip of each anchor kind through `encodeAnchor`/`decodeAnchor`; `parseAnchor rejects bad input` (`{ kind: "weakness" }`, `{ kind: "move", gameId: "g", ply: -1 }`, `{ kind: "report_section", reportId: "x", section: "overview" }`, extra keys, a string, `null`); `coachHref` gives `/app/coach/new?account=<id>&tc=rapid&anchor=<encoded>`.
  - `copy-parity.test.ts`: every Rust-mirrored string in `CHAT_COPY` and every `fallbackTitle` appears verbatim in `backend/crates/ai/src/chat/limits.rs` or `summary.rs`.
  - `threads.test.ts` (real Postgres; users A and B both linked to one account):
    - `thread of another user is not found`: B's `getThread` of A's thread is null.
    - `list shows only the user's threads`, newest first.
    - `delete of another user's thread deletes nothing`, and the owner's delete removes the messages by cascade.
    - `thread replays stored chunks`: a `streaming` assistant message with two seeded `chat.chunk` events for A gives `streamed` of length 2 in `seq` order.
    - `streamed chunks are the owner's only`: a chunk event stored with B's `user_id` for A's message is not in A's `streamed`.
    - `engine checks are listed per message` from `interactive_evals.message_id`.
  - `post.test.ts`:
    - `posts the user turn and a pending answer`: seq 1 `user` `done` and seq 2 `assistant` `pending`, one `coach-chat` job with payload `{"messageId": <assistant id>}` and key `chat:<id>`.
    - `second post while answering is busy`, and two concurrent posts give one `posted` and one `busy`.
    - `blank question is empty`; `2001 code points is too long` (a 2000-character question with an emoji counted by code points passes).
    - `post to another user's thread is not found`; `post after unlinking is not_linked` (read and delete still work).
  - `allowance.test.ts`: two `done` answers with a disclosure and one `streaming` this week give `used: 3` and `exhausted.reason: "weekly_cap"`, resetting next Monday 00:00 UTC; a `done` no-data answer (no disclosure) and a `failed` one are not counted; last week's answers are not counted.
  - `routes.test.ts`: cross-site POST 403, signed-out 401, 409 on busy, 404 on a foreign thread, 201 on create with a first question.
  - `segment-href.test.ts`: a `move` at ply 5 gives `/app/games/<id>?ply=5`, matching the ply M5a's review page highlights for that SAN (checked against `games.middlegameBlunder` from `@ci/ui/fixtures`); `line` gives null.
  - `scope-switch.test.tsx`: one option renders nothing; choosing an option writes `account` and `tc`.
  - `coach-events.test.ts`: `chunks append in seq order and dedupe`; `ready marks the stream done with replace`; `report events invalidate only that account's report key`; `malformed payloads are ignored`; and `applyEvent` still handles `game.analysed` (M5a behaviour unchanged).
- [ ] **Step 2:** Run `$WEB pnpm --filter @ci/web exec vitest run test/coach test/realtime`. Expected: FAIL.
- [ ] **Step 3:** Implement. ADR 0070 records R5 and R6, the one-transaction post, the chunk replay, the copy parity, the allowance count, and the follow-up for M7 (`AI_PLAN` comes from entitlements).
- [ ] **Step 4:** Run the same command (PASS), then lint and typecheck.
- [ ] **Step 5:** Commit `feat(web): coach chat threads, posting and realtime coach events`.

**Reviewer gate:** the reviewer mutation-checks every `user_id` filter (removing any one must fail a test), diffs `postUserMessage` against `post_user_message`, and confirms the web env holds no sidecar URL or token.

### Task 6: Report page `/app/report` and the overview's "What to work on"

**Files:**
- Create: `apps/web/src/app/(app)/app/report/{page.tsx,loading.tsx,report-view.tsx,rebuild-button.tsx,reveal.ts,_sections/{snapshot.tsx,areas.tsx,weaknesses.tsx,openings.tsx,climb-plan.tsx,changes.tsx,state-banner.tsx}}`, `apps/web/test/pages/{report.test.tsx,work-on.test.tsx}`, `docs/decisions/0073-coach-surfaces-and-report-page.md`
- Modify: `apps/web/src/app/(app)/app/_overview/headline.tsx` (`WorkOn` takes a `LeadFinding`), `apps/web/src/app/(app)/app/page.tsx` (loads `leadFinding`)

**Interfaces:**
- Consumes: Task 1 (`resolveCoachScope`, `listAreaScores`, `leadFinding`, `seriesLabel`), Task 2 (`getReport`, `changeNotes`, `POST /api/coach/report`), Task 3 (`SegmentText`, `AiDisclosure`, `CoachState`), Task 4 (`WeaknessRow`, `ExemplarBoard`, `AreaScore`), Task 5 (`coachHref`, `segmentHref`, `CoachScopeSwitch`, `LiveRefreshOn`, `queryKeys.report`), M5a `requireUser`.
- Produces:
  - The page, top to bottom: the state banner (from `ReportState`, below); the snapshot (`computed.player` rating, band and time class; the basis line "Based on 214 analysed rapid games from 3,912 imported", with "backfill still running" when `backfillComplete` is false; the seven `AreaScore`s from `computed.areaScores`, an insufficient area showing "Need N more games"; the `AiDisclosure`); the overview section (headline, summary and strengths); the seven area notes (`areas` items by area, linked to `/app/areas/<area>`); "What's costing you games" (`computed.weaknesses` by rank, each with its `WeaknessRow`, the coach's item for that `weaknessId`, up to 3 `ExemplarBoard`s, and an "Ask your coach" link with a `weakness` anchor); openings (`computed.openings` with verdicts Keep, Fix, Learn deeper, Replace and the coach's item); the climb plan (at most 3 focuses with why, habit and drills listed by name only; drills are out of scope, so no "train this" button); "New this month" and "Fixed this month" (`newThisPeriod`/`fixedThisPeriod`, with the ready change notes); and the `dropped` count ("2 statements were removed because they couldn't be checked against your games").
  - A section whose content is null shows the computed part and a `CoachState` `writing` ("Your coach is writing this section") while the run is building, or "Your coach's notes for this section couldn't be checked, so they're not shown" when dropped.
  - State banner copy (exact): `none` → "Your coaching report hasn't been built yet." with "Build my report"; `building` → `ProgressPlaceholder` "Building your report: 2 of 5 sections written" (with "Waiting for the batch, usually within an hour" for `batch`, and "Your coach is busy; retrying at 14:05 UTC" for `retryAt`); `ready` `computed_only` with `aiOff: "no_api_key"` → "Your coach's notes are off for now. The numbers below are computed from your games."; with `aiOff: "budget"` → "Your coach has reached this month's spending limit, so this report shows the computed numbers only."; `failed` `plan_interval` → "You can rebuild this report on 22 Dec 2026."; `failed` `no_input` → "There isn't enough analysed data for a report yet."; `failed` `budget` → as the budget line; any other `failed` → "The last rebuild failed. Your previous report is still shown." (or, with no previous report, "Building your report failed. Try again.").
  - `RebuildButton({ scope, nextManualAt, building })`: posts to `/api/coach/report`, toasts "Rebuilding your report. Sections appear as they're written.", is disabled while building, and before `nextManualAt` says "Rebuild available on 22 Dec 2026".
  - Realtime: `LiveRefreshOn({ keys: [queryKeys.report(scope.accountId)] })`, so each `report.section.ready` re-renders the page and the new section assembles in.
  - Report reveal (§11): on the first view of a report version on this device (`localStorage` key `ci.report.seen`, try/catch), sections assemble with a staggered `spring.page` and the weaknesses resolve in rank order; later views and reduced motion are instant; any key or pointer press ends it.
  - `WorkOn({ lead }: { lead: LeadFinding })`: `weakness` → its label, `weaknessRateText`, the basis and a link to `/app/weaknesses/<seriesKey>?account=…&tc=…`; `none` → "Nothing stands out above players at your level across 214 analysed games yet."; `building` → the current copy.

- [ ] **Step 1: Write the failing tests.**
  - `report.test.tsx` (the view rendered from seeded payloads via Task 2's helper): `renders every report state without NaN` (each row of Task 2's table renders its banner copy above, with no `NaN|undefined|Infinity|null` in the HTML); `partial report keeps the computed part of a dropped section`; `move segments link to the review ply`; `insufficient area says need more`; `shows the AI disclosure only when a section is ready`; `rebuild is disabled before the interval ends and shows the date`.
  - `work-on.test.tsx`: the three `LeadFinding` kinds render their copy and link.
- [ ] **Step 2:** Run `$WEB pnpm --filter @ci/web exec vitest run test/pages/report.test.tsx test/pages/work-on.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Load `design-system` and `chess-ui`, then implement. The layout continues the M5a overview sheet (R7). ADR 0073 records the coach routes (`/app/report`, `/app/weaknesses`, `/app/weaknesses/[seriesKey]`, `/app/areas/[area]`, `/app/progress`, `/app/coach`), R7, the section order, the state copy, the reveal and the "no train this button" decision.
- [ ] **Step 4:** Run the same command. Expected: PASS.
- [ ] **Step 5:** Run CL on `/app/report` in five states (complete, partial while building, computed only with AI off, never built, failed with a previous report) and on `/app` (the lead finding), seeded with Task 1 and Task 2 helpers against the throwaway `ci-m5b-e2e` stack. Log it in `packages/ui/qa/log/report.md`.
- [ ] **Step 6:** Commit `feat(web): coaching report page with progressive sections`.

**Reviewer gate:** the critic's `pass`; the reviewer greps every state's HTML for `NaN|undefined|Infinity|null|{{`, and checks that no section is presented as the coach's when its row is not `ready`.

### Task 7: Weaknesses `/app/weaknesses` and `/app/weaknesses/[seriesKey]`

**Files:**
- Create: `apps/web/src/app/(app)/app/weaknesses/{page.tsx,loading.tsx,weaknesses-view.tsx}`, `apps/web/src/app/(app)/app/weaknesses/[seriesKey]/{page.tsx,not-found.tsx,weakness-view.tsx,exemplars.tsx}`, `apps/web/test/pages/weaknesses.test.tsx`

**Interfaces:**
- Consumes: Task 1 (`resolveCoachScope`, `listWeaknesses`, `getWeakness`, `difficultyStatus`), Task 2 (`changeNotes`), Task 3 (`SegmentText`, `AiDisclosure`), Task 4 (`WeaknessRow`, `ExemplarBoard`, `KpiTrendGrid` for the weekly series), `WeaknessTimeline` (`@ci/ui/timeline`), Task 5 (`coachHref`, `segmentHref`, `CoachScopeSwitch`, `LiveRefreshOn`, `queryKeys.notes`).
- Produces:
  - List: weaknesses grouped as "Costing you games" (state `active`, `emerging` or `relapsed`), "Getting better" (`improving`) and "Under control" (`resolved`, `not_an_issue`), each by priority, each a `WeaknessRow` linked to its detail. The page opens with the basis and the difficulty line ("Difficulty adjustment is waiting for Maia on 112 of 3,040 positions"; omitted when `pendingMaia` is 0). Empty → "No weaknesses measured yet. They appear after about 20 analysed games."
  - Detail (`seriesKey` URL-decoded, `notFound()` when `getWeakness` is null): the label and state; the estimated rate with its stored interval and counts; the band comparison ("Band median 2.1%"; omitted when `bandMedian` is null, with "No band reference yet"); the onset ("Started around 14 Oct 2026, between 2 Oct and 20 Oct", only from an onset row); the `WeaknessTimeline` for this series; the weekly series as a one-item `KpiTrendGrid` (`minN` = 20 chances per week); the latest ready change note with its disclosure ("Possible causes" wording comes from the stored text); up to 5 `ExemplarBoard`s with "Examples from your report of 12 Nov 2026" and links to the review ply; and "Ask your coach about this" (`weakness` anchor with `contractId`, or no anchor for non-`W` series).

- [ ] **Step 1: Write the failing tests** (`weaknesses.test.tsx`, views rendered from seeded rows): `groups by state`; `empty list says when weaknesses appear`; `difficulty line counts pending_maia and hides at zero`; `unknown series key is not found` (page call with `W99` triggers `notFound`); `detail shows stored interval and counts`; `onset only from an onset row`; `no report means no exemplars, with the honest line`; `ask link carries the weakness anchor`; and no `NaN|undefined|Infinity|null` in any view.
- [ ] **Step 2:** Run `$WEB pnpm --filter @ci/web exec vitest run test/pages/weaknesses.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Load `design-system` and `chess-ui`, then implement. The weakness card expands into its detail with a shared-element transition (View Transitions API with a Motion fallback, §11 continuity).
- [ ] **Step 4:** Run the same command. Expected: PASS.
- [ ] **Step 5:** Run CL on `/app/weaknesses` (rich, empty) and `/app/weaknesses/W1` (sufficient with exemplars, early read without a report). Log it in `packages/ui/qa/log/weaknesses.md`.
- [ ] **Step 6:** Commit `feat(web): weaknesses list and weakness detail pages`.

**Reviewer gate:** the critic's `pass`; the reviewer checks every exemplar's arrows against its FEN for one White and one Black position.

### Task 8: Skill-area pages `/app/areas/[area]`

**Files:**
- Create: `apps/web/src/app/(app)/app/areas/[area]/{page.tsx,not-found.tsx,area-view.tsx}`, `apps/web/src/app/(app)/app/areas/page.tsx` (redirects to `/app/areas/openings` keeping the scope params), `apps/web/test/pages/areas.test.tsx`

**Interfaces:**
- Consumes: Task 1 (`getArea`, `resolveCoachScope`), Task 2 (`getReport` for the area's coach note and exemplars), Task 3, Task 4 (`AreaScore`, `KpiTrendGrid`, `WeaknessRow`, `ExemplarBoard`), `WeaknessTimeline`, Task 5 (`coachHref`, `segmentHref`, `CoachScopeSwitch`).
- Produces: one page per `Area` (`openings`, `middlegame`, `endgame`, `tactics`, `calculation`, `time`, `mindset`; anything else is `notFound()`), with a tab row linking the seven. Top to bottom: the area score (`AreaScore`, or "Need N more games for an endgame score" from the report's `areaScores.needMore`, or "Not enough games for a score yet" without it; Mindset says "Mindset is coaching advice, not a score"); the coach's note for the area from the report's `areas` section (`SegmentText` and disclosure, or the section's honest state); the area's KPIs as a `KpiTrendGrid` (catalogue labels and units, each with n; insufficient KPIs say "Need N more"); the area's weaknesses as `WeaknessRow`s and their `WeaknessTimeline` (§12b "Each area page shows the timeline for its own weaknesses"); up to 3 example positions from the report's ranked weaknesses in this area; and "Ask your coach about your <area>" (a `report_section` anchor on `areas` when a report exists).

- [ ] **Step 1: Write the failing tests** (`areas.test.tsx`): `unknown area is not found`; `mindset says it has no score`; `null score says not enough games` and never renders 0; `needMore from the report is shown`; `kpis carry n and need-more`; `area weaknesses only`; no `NaN|undefined|Infinity|null`.
- [ ] **Step 2:** Run `$WEB pnpm --filter @ci/web exec vitest run test/pages/areas.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Load `design-system`, `chess-ui` and `dataviz`, then implement.
- [ ] **Step 4:** Run the same command. Expected: PASS.
- [ ] **Step 5:** Run CL on `/app/areas/endgame` (scored), `/app/areas/time` (insufficient) and `/app/areas/mindset`. Log it in `packages/ui/qa/log/areas.md`.
- [ ] **Step 6:** Commit `feat(web): seven skill-area pages`.

**Reviewer gate:** the critic's `pass`, and a check that the seven pages share one layout with no card grid (B4).

### Task 9: "Ask your coach" chat `/app/coach`

**Files:**
- Create: `apps/web/src/app/(app)/app/coach/{page.tsx,layout.tsx,thread-list.tsx}`, `apps/web/src/app/(app)/app/coach/new/page.tsx`, `apps/web/src/app/(app)/app/coach/[threadId]/{page.tsx,not-found.tsx,thread-view.tsx,use-thread.ts,anchor-card.tsx}`, `apps/web/test/pages/coach.test.tsx`, `apps/web/test/pages/use-thread.test.tsx`

**Interfaces:**
- Consumes: Task 5 (all of it, including `segmentHref`), Task 3 (`ChatMessage`, `ChatComposer`, `AiDisclosure`, `CoachState`), Task 1's `resolveCoachScope`.
- Produces:
  - `/app/coach`: the thread list for the scope's account (title, anchor kind, "12 Nov 2026"), "New conversation", the allowance line ("2 of 3 coach messages left this week, resets Monday") and the AI disclosure note. Empty → "Ask about any of your games, moves, openings or weaknesses."
  - `/app/coach/new?account&tc&anchor`: decodes the anchor (dropped silently when invalid), shows its `AnchorCard` (the position as a small board for `position` and `move`, the weakness label, the report section name, or the game caption), and the composer. Sending creates the thread with its first question and navigates to it.
  - `/app/coach/[threadId]`: the messages (`ChatMessage`), the composer, "Delete conversation" (a confirm dialog; then back to the list with the toast "Conversation deleted"). `notFound()` for a foreign or unknown id.
  - `useThread(threadId, initial: Thread)`: holds the thread, subscribes to `queryKeys.chatStream(messageId)` of the unfinished answer, shows its chunks live, refetches `GET /api/coach/threads/[id]` on `done` (replacing the streamed text when `replace`), and shows `failureCopy(reason)` on failure. While an answer is unfinished the composer is disabled with "Your coach is answering". When `chatAllowance.exhausted`, the composer is disabled with `capText`.
  - Status honesty: while the stream mode is not `sse`, the page says "Updates every few seconds" (M4 `useRealtime`), never "live".

- [ ] **Step 1: Write the failing tests.**
  - `use-thread.test.tsx` (with a `QueryClient` and `applyCoachEvent`): `chunks stream into the pending answer`; `ready refetches and replaces the streamed text`; `failed shows the failure copy` (`weekly_cap` gives "You've used this week's coach messages. They reset on Monday."); `composer is disabled while answering`.
  - `coach.test.tsx`: `invalid anchor is dropped`; `anchor card shows the position for a move anchor`; `exhausted allowance disables the composer with the reset copy`; `foreign thread is not found`; `delete asks for confirmation`; no `NaN|undefined|Infinity|null|{{`.
- [ ] **Step 2:** Run `$WEB pnpm --filter @ci/web exec vitest run test/pages/coach.test.tsx test/pages/use-thread.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Load `design-system` and `chess-ui`, then implement. A finished answer settles with the `pen` spring; streamed paragraphs never reflow the composer.
- [ ] **Step 4:** Run the same command. Expected: PASS.
- [ ] **Step 5:** Run CL on `/app/coach` (empty, with threads), `/app/coach/new` (move anchor) and a thread mid-answer, answered and denied, seeded with Task 5's `seedThread` and `seedChunk` against the throwaway stack. Log it in `packages/ui/qa/log/coach.md`.
- [ ] **Step 6:** Commit `feat(web): ask your coach chat with streaming answers`.

**Reviewer gate:** the critic's `pass`; the reviewer runs one full keyboard pass (compose, send, delete) and checks that a reload mid-answer keeps the streamed paragraphs.

### Task 10: Progress page `/app/progress`

**Files:**
- Create: `apps/web/src/app/(app)/app/progress/{page.tsx,loading.tsx,progress-view.tsx,weekly-reviews.tsx,before-after.tsx}`, `apps/web/test/pages/progress.test.tsx`

**Interfaces:**
- Consumes: Task 1 (`kpiTrends`, `listWeaknesses`, `getWeakness` histories), Task 2 (`listWeeklyReviews`, `getReport` for report dates as events), Task 3 (`SegmentText`, `AiDisclosure`, `CoachState`), Task 4 (`KpiTrendGrid`), `WeaknessTimeline`, Task 5 (`LiveRefreshOn`, `queryKeys.weekly`, `CoachScopeSwitch`, `segmentHref`).
- Produces, top to bottom:
  - **Before and after** (§10a.4, first): for each weakness with a `transition` row into `improving`, `resolved` or `relapsed`, its `rateBefore` and `rateAfter` from that row, with the state and date ("Hung material: 4.1% before, 2.6% after 14 Oct 2026, detected improving"). The wording follows §12a: "detected", never "fixed by training". Empty → "No change detected yet. Changes need enough games on both sides to be called."
  - **Rating and KPIs:** the `rating` trend, then small multiples for accuracy (overall and by phase), `blunders_per_game`, `blunder_rate_low_clock`, `conversion_rate`, `resilience_rate` and `missed_punish_rate`, on one shared time axis (the scope's first to last KPI week), with report dates as events. The rating is labelled "Your recorded rating; it moves for many reasons" and never linked to training.
  - **Weakness timeline:** `WeaknessTimeline` with one swimlane per measured weakness and its onset markers.
  - **Weekly reviews:** each ready review's headline, process, outcomes (with their computed labels), exemplars and next week, with the disclosure; pending as "Your weekly review is being written"; dropped or failed as "This week's review couldn't be checked against your games, so it isn't shown." Empty → "Weekly reviews appear here once your coach writes them." (see Spec gap G2).

- [ ] **Step 1: Write the failing tests** (`progress.test.tsx`): `before and after comes from the transition row`; `no transition gives the honest empty line`; `rating is labelled without causal claim` (no "because", "thanks to" or "improved by" in the rating block); `kpi grid shares one range`; `pending weekly review shows the writing state`; `empty weekly reviews say when they appear`; no `NaN|undefined|Infinity|null|{{`.
- [ ] **Step 2:** Run `$WEB pnpm --filter @ci/web exec vitest run test/pages/progress.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Load `design-system`, `chess-ui` and `dataviz`, then implement.
- [ ] **Step 4:** Run the same command. Expected: PASS.
- [ ] **Step 5:** Run CL on `/app/progress` (rich, with one weekly review; and a new account with two weeks of data). Log it in `packages/ui/qa/log/progress.md`.
- [ ] **Step 6:** Commit `feat(web): progress page with KPI trends, timeline and weekly reviews`.

**Reviewer gate:** the critic's `pass`, plus the reviewer's check of every causal word on the page against §12a.

### Task 11: Navigation, end-to-end, quality sweep, baselines and owner walkthrough

**Files:**
- Create: `apps/web/e2e/coach.spec.ts`, `apps/web/e2e/helpers/{seed-coach.ts,simulate-worker.ts}`, `apps/web/test/e2e-coach-seed-guard.test.ts`, `apps/web/test/nav.test.ts`, `packages/ui/qa/log/m5b-final.md`, `docs/decisions/0074-coach-e2e-and-simulated-worker.md`
- Modify: `apps/web/src/app/(app)/_shell/nav.ts`, `apps/web/src/app/(app)/_shell/nav-command-menu.tsx`, `apps/web/e2e/{game-pages-quality,game-pages-visual}.spec.ts` (route loops gain the coach routes), `apps/web/e2e/__screenshots__/**`, `docs/decisions/README.md` (indexes 0068–0074)

**Interfaces:**
- Consumes: everything above; M5a `seedE2eGames`, `isE2eUsername` and its guard; Tasks 1, 2 and 5 test helpers (imported, not copied).
- Produces:
  - `navItems` gains, in this order after "Games": `{ label: "Report", href: "/app/report", keywords: ["coaching", "plan", "summary"] }`, `{ label: "Weaknesses", href: "/app/weaknesses", keywords: ["mistakes", "leaks"] }`, `{ label: "Progress", href: "/app/progress", keywords: ["trends", "kpi", "weekly"] }`, `{ label: "Coach", href: "/app/coach", keywords: ["ask", "chat", "question"] }`. The command menu adds a "Skill areas" group with the seven area pages and an "Ask your coach" action (opens `/app/coach/new`). At 360px the header nav stays usable (overflow into the existing menu, no horizontal scroll).
  - `seedE2eCoach(): Promise<{ accountId: string; threadId: string; streamingMessageId: string }>`: refuses a database with real accounts (M5a's guard, same message), then seeds for the adult e2e user: profile rows (Task 1 helper; 8 weaknesses across states, KPI weeks 2026-09 to 2026-11 with one null week, area scores with one null and Mindset), a complete report from `report-payload.example.json` created 40 days before the run (so a rebuild is allowed) plus an older version, one ready and one pending weekly review, one ready change note, and one thread with a done answer.
  - `simulateWorker(db, step)`, with `step` one of `{ kind: "report-section"; runId; section: SectionKey }` (writes the section row from the fixture, updates the report status, emits `report.section.ready`), `{ kind: "chat-chunk"; messageId; seq; text }` (emits a user-owned `chat.chunk`), `{ kind: "chat-done"; messageId; replace?: boolean }` (stores segments and disclosure, `done`, emits `chat.message.ready`) and `{ kind: "chat-failed"; messageId; reason }`. It writes only what the R3 jobs write, with the owner's `user_id` on chat events, and makes no AI call.
  - `coach.spec.ts`:
    1. `/app/report` shows the snapshot, seven area scores, ranked weaknesses with boards and the disclosure.
    2. "Rebuild report" shows the building state; `simulateWorker` writes `overview` then `weaknesses`, and each appears without a reload.
    3. A weakness row opens its detail with the timeline, the counts and an example board; "Ask your coach about this" opens `/app/coach/new` with the anchor card.
    4. Sending a question shows "Your coach is reading your games"; two simulated chunks appear in order; `chat-done` with `replace` swaps in the final text; a reload mid-answer (between the chunks) still shows the first chunk.
    5. A second user linked to the same account gets the not-found page for the first user's thread, and their open event stream receives no `chat.chunk` (checked from the page's event log).
    6. A `weekly_cap` denial shows the cap copy and disables the composer.
    7. `/app/areas/mindset` says it has no score; `/app/areas/kingside` is not found.
    8. `/app/progress` shows before-and-after first, the KPI grid with a gap for the null week, and the weekly review.
    9. The command menu (Ctrl+K) finds "Endgame" and opens `/app/areas/endgame`.
  - `nav.test.ts`: the nav order, and `isCurrent("/app/weaknesses/W1", "/app/weaknesses")`.

- [ ] **Step 1:** Write `nav.test.ts`, `e2e-coach-seed-guard.test.ts` (`$WEB`: a `linked_accounts` row `mtsgambit` makes `seedE2eCoach` throw the refusal message and write nothing) and `coach.spec.ts`. Run the unit tests. Expected: FAIL.
- [ ] **Step 2:** Implement the nav, the command-menu entries, `seed-coach.ts` and `simulate-worker.ts`. Run the unit tests (PASS), then `pnpm --filter @ci/web e2e coach game-pages-quality` against `ci-m5b-e2e` (PASS). Fix failures in the owning task's files.
- [ ] **Step 3:** Run `pnpm design:check`, `pnpm lint`, `pnpm typecheck`, `pnpm test`, `$WEB pnpm --filter @ci/web test` and `$RUST cargo test --locked -p profile-core --test catalogue_snapshot`. Expected: all pass, `0 findings`.
- [ ] **Step 4:** Run the final CL across `/app`, `/app/report`, `/app/weaknesses/W1`, `/app/areas/endgame`, `/app/progress` and a coach thread. A fresh critic looks for inconsistency between the coach pages, with M5a's pages and with `/styleguide`. Log it in `m5b-final.md`.
- [ ] **Step 5:** Generate visual baselines in the Linux `qa` container (M4 T28 Step 3) for the six routes, then rerun without `--update-snapshots`. Expected: zero diffs.
- [ ] **Step 6:** Tear down `ci-m5b-e2e`. Owner walkthrough on the default project (read-only for the web; never `down -v`), recorded in the ledger without the owner's data: start `web` and the worker with the sidecar (CLAUDE.md "AI needs the `ai-sidecar` service"), sign in, open `/app/report`, press "Build my report" once, and watch the sections arrive; ask the coach one question. This is the owner's live run, not a test, and it spends real tokens only with the owner's go-ahead.
- [ ] **Step 7:** Index ADRs 0068–0074 in `docs/decisions/README.md`. ADR 0074 records `simulateWorker`, the e2e seeds and the baselines. Commit `test(web): coach surfaces e2e, quality checks and visual baselines`.

**Reviewer gate:** a whole-branch review of `m5b/base` (most capable model) before merging into the integration branch. It covers the Review Focus list, the honesty rules on every rendered state and the privacy tests.

---

## Spec gaps for a coordinator ruling

- **G1, beta entitlements:** the worker wires `DefaultEntitlements` (plan Free) for AI. Every beta tester therefore gets 3 coach messages a week, 3 engine checks a week and one manual report per 30 days, and the report job fails `plan_interval` inside that window. M5b mirrors Free honestly (R4, R6). If the beta should be Pro-like, the coordinator rules it, and a small Rust change (entitlements from an env var, e.g. `AI_DEFAULT_PLAN`) plus the web's `AI_PLAN` constant follow.
- **G2, weekly reviews never get written:** production wires `NullWeeklyProvider`, so no `weekly_reviews` row is created and the progress page always shows its empty state. Building the `WeeklyReviewInput` provider is R3/M7 Rust work and is outside this plan.
- **G3, first report:** nothing in the backend calls `request_report`; only the web's "Build my report" starts one. Should `build-profile` request the first report automatically after onboarding (§10.2's "first insights")?

## Self-review

- **Spec coverage:** §10.3 report → Task 6 (snapshot, seven areas, strengths, ranked weaknesses with boards, openings verdicts, climb plan; "train this" is out of scope with drills). §8b area pages → Task 8. §12b UI → Tasks 7, 8 and 10 (timeline, onset, states; the heads-up for `emerging` is the "Costing you games" group). §10.6, §10a.4 and §12a plotting → Task 10. §10a.9 chat → Tasks 5 and 9 (anchors, threads saved and deletable, limits shown, safety and validation stay in Rust). §13a progressive UI → Tasks 5 and 6 (events patch or invalidate one key, designed placeholders, toasts, no blocking). §14 AI disclosure → Tasks 3, 6, 7, 9 and 10. The overview slot → Task 6. Nav and command menu → Task 11. Out of scope as ruled: landing, quick diagnosis, WASM review, drills and the opening explorer (opening lines are shown in the report but not linked).
- **Type consistency:** `CoachScope`, `WeaknessSummary`, `KpiPoint`, `ReportState`, `ReportPayload`, `Segment`, `ChatAnchor` and `Thread` are each defined once (Tasks 1, 2 and 5) and named unchanged later; `@ci/ui` takes plain structural props.
- **Proportion:** interfaces, exact copy and test assertions only; no bodies except the state table and the SQL rules the backend pins.
