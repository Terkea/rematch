# Profile Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn analysed games into a full player profile: per-move tactical features, weaknesses W1–W12 with shrunk posteriors and priorities, rating-band references and our own opening explorer from the CC0 Lichess database, a repertoire tree with verdicts, seven area scores, append-only KPI snapshots, a weakness timeline with change points, a priority-driven analysis scheduler for large histories, and a `build-profile` job. Its final output is the `CoachingInput` that Milestone 3 consumes.

**Architecture:** All chess and statistics logic is pure TypeScript in `@ci/chess-core`, split into folders that each have one owner lane and one barrel. Engine-side work (MultiPV, the null-move threat probe, Syzygy, difficulty) is an **enrichment layer** in `apps/worker`. It has its own protocol id, so `sf19-n300k-v1` evals stay as they are. Reference data (band references and the opening explorer) is built by worker jobs that stream one month of the Lichess dump from a git-ignored `data/` directory. A `build-profile` job assembles everything per account × time class, appends snapshots and state transitions, and publishes `profile.updated` and `weakness.changed`.

**Tech Stack:** Node 22 (≥ 22.15 for `node:zlib` zstd), TypeScript 5.9, chess.js 1.4, Vitest 5, Drizzle 0.45 + postgres.js, BullMQ 6, Postgres 17, Redis 7, Stockfish 19, Syzygy 3-4-5 tables, Docker Compose. Conditionally, Python 3.12 + FastAPI + maia2 for `services/maia` (Task 21).

**Spec:** `docs/superpowers/specs/2026-11-21-chess-improver-design.md` (§8, §8a, §8b, §8c, §8 cont., §12a, §12b, §13a, §14, §16, §17) and `docs/research/improvement-engine.md` §4.4 and §6 (normative). Roadmap row 2. Builds on M1 (`docs/superpowers/plans/2026-11-21-01-foundation-and-analysis.md`, including Task 12b). Consumer: `docs/superpowers/plans/2026-11-21-03-ai-coaching.md` Task A1.

## Global Constraints

- Local stack is Docker Compose. DB-backed tests run in Docker: `packages/db` tests use `chess_test` (`DATABASE_URL_TEST`), worker tests use `chess_worker_test` via `getWorkerTestDb()` (`WORKER_DATABASE_URL_TEST`). When `IN_DOCKER=1`, a missing URL, missing Syzygy files or a missing Stockfish binary makes the test **fail, not skip**.
- Package scope `@ci/*`. ESM only, TS `strict` + `noUncheckedIndexedAccess`, relative imports end in `.js`, and no `__dirname` (use `new URL(..., import.meta.url)`).
- ADR 0026: workspace packages resolve source through the `development` export condition. M2 adds **no new workspace TS package**. Pure code goes in `@ci/chess-core`. `services/maia` is Python and has no `package.json`. M2 adds no npm dependency, so the lockfile does not change.
- `ANALYSIS_PROTOCOL = "sf19-n300k-v1"` (300_000 nodes, MultiPV 1) stays unchanged for `position_evals`. Enrichment is versioned separately: `ENRICHMENT_PROTOCOL = "enr-v1"`, `MULTIPV_PROTOCOL = "sf19-n300k-mpv3-v1"`, `THREAT_PROTOCOL = "sf19-n50k-null-v1"`, and tablebase probes through a **separate** engine pool. The main pool never sets `SyzygyPath` (ADR 0030).
- Thresholds (improvement-engine §6.2): eligible = `|winBefore − 50| < 40`; decisive error = `deltaW ≥ 15`; inaccuracy-grade = `deltaW ≥ 5`; critical = MultiPV best − second ≥ 15 W%; material values P1 N3 B3 R5 Q9.
- Bands on the Lichess-equivalent scale: B0 < 900, B1 900–1299, B2 1300–1699, B3 1700–1999, B4 2000–2299, B5 ≥ 2300. chess.com ratings map through `CHESSCOM_TO_LICHESS_OFFSET` (provisional). Ratings from different platforms are never mixed in one series.
- Every interval is at the 80% level (`Z80 = 1.2815515655446004`). Rates use Wilson intervals, means use a bootstrap (1000 resamples), posteriors use Beta quantiles, and P_imp uses 10,000 Monte Carlo draws.
- Determinism: chess-core never calls `Math.random`. Every stochastic function takes a seed (`createRng`).
- Licensing (§14): we write our own code from documented rules. Nothing is copied from `cook.py`, `lila-openingexplorer`, `lichess-org/database` scripts, chessops or chessground (GPL/AGPL). Data comes only from CC0 Lichess sources. Maia is used only if Task 21's licence check passes.
- Large data is never committed. Dumps and tablebases go under the git-ignored `data/` directory (`DATA_DIR`, default `<repo>/data`, `/app/data` in the container). Downloads are verified by SHA-256 before use. Tests use fixtures generated in the test, or committed fixtures of 50 KB or less.
- Lichess DB processing streams **one month** (configurable `month`, `maxGames`, `sampleRate`), never the archive. Usernames from the dump are stored only as `sha256(salt + lower(name))` with a per-build salt.
- State lives in Postgres and is enqueued after commit. Events go through M1 12b `publishEvent` after the commit.
- ADR numbers are pre-assigned per task (0030–0043) to avoid collisions. Only Task 26 edits `docs/decisions/README.md`.
- Commits follow Conventional Commits and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Prerequisite:** M1 Task 12b is merged (`publishEvent`, `userEvents`, `importRuns`, `importCursor`, heartbeats, `reconcile`, `createCircuitBreaker`).

## Parallel lanes and file ownership

| Hot spot | Sole owner |
|---|---|
| `packages/chess-core/src/index.ts`, `src/profile/**` | Task 1 |
| `packages/db/src/{schema.ts,profile-schema.ts,index.ts,test-utils.ts}`, `drizzle.config.ts`, `packages/db/drizzle/*`, `packages/db/test/setup.ts`, `apps/worker/test/db.ts`, `.gitignore` | Task 2 |
| `biome.json` | Task 12 |
| `docker-compose.yml`, `.env.example`, `apps/worker/src/engine/engine-pool.ts` | Task 20, then Task 21 (same lane, in sequence) |
| `apps/worker/src/engine/protocol.ts` | Task 22 |
| `apps/worker/src/jobs/analyse-game.ts` | Task 22, then Task 24 |
| `apps/worker/src/jobs/import-account.ts`, `apps/worker/src/jobs/reconcile.ts`, `apps/worker/src/queue/queues.ts` | Task 23 (Task 25 adds new queues to `queues.ts` afterwards) |
| `apps/worker/src/start-workers.ts`, `main.ts`, `events/publish.ts` | Task 25 |
| `apps/worker/src/cli.ts`, `docs/decisions/README.md`, `CLAUDE.md` | Task 26 |

Each chess-core folder has a barrel `src/<folder>/index.ts`, created empty by Task 1 and then edited only by its lane.

| Lane | Folders / files | Tasks (in order) | Depends on |
|---|---|---|---|
| **W0 contracts** | `chess-core/src/profile`, `packages/db` | 1, 2 (parallel) | M1 |
| **P1 detectors** | `chess-core/src/{board,motifs,features,weakness}` | 3 → 4 → 5 → 6 → 7 → 8; 9 after 5 and 6 | 1 |
| **P2 data** | `chess-core/src/openings`, `apps/worker/src/reference/**`, `apps/worker/src/openings/**`, `jobs/build-explorer.ts`, `jobs/build-band-reference.ts` | 10 → 11; 12 (parallel); 13 after 11, 12, 15; 14 after 7, 8, 15, 19, 22 | 1, 2 |
| **P3 statistics** | `chess-core/src/{stats,timeline,kpi}` | 15 → 16; 15 → 17 → 18 (18 also after 16); 19 after 15 | 1 |
| **P4 services** | `apps/worker/src/{tablebase,difficulty}/**`, `services/maia/**` | 20 → 21 | 1, 2 |
| **P5 pipeline** | `apps/worker/src/{enrichment,scheduler}/**` | 22; 23 (parallel) | 1, 2 |
| **I integration** | `apps/worker/src/profile/**`, `jobs/build-profile.ts` | 24 (after 5, 7, 8, 9, 11, 12, 22) → 25 (after all lanes) → 26 | all |

Waves: (0) 1, 2 · (1) 3, 6, 10, 12, 15, 20, 22, 23 · (2) 4, 11, 16, 17, 19, 21 · (3) 5, 18 · (4) 7, 9, 13 · (5) 8 · (6) 14, 24 · (7) 25 · (8) 26.

## Contract mapping to M3

`CoachingInput` (Task 1, `packages/chess-core/src/profile/coaching-input.ts`) mirrors M3 Task A1's zod schema **field for field, with the same names**: `inputVersion: "coaching-input-v1"`, `accountId`, `platform`, `timeClass`, `timeControl`, `band`, `rating`, `generatedAt`, `basis`, `areaScores`, `metrics`, `weaknesses`, `openings`, `changes`. The same goes for `ExemplarPosition`, `Metric`, `RankedWeakness` (including `pImp`), `OpeningVerdict` (including `alternatives` and `examples`), `AreaScore`, `TimelineChange` (including `examples` with `side`), `CandidateCause` and `Basis`. Differences:

| M2 | M3 A1 | Note |
|---|---|---|
| `strengths: StrengthEntry[]` | — | **Added by controller ruling.** M3 A1's strict schema must add `strengths` (shape in Task 1). |
| internal `WeaknessKey` (`"W3.shallow"`, `"W7.traps"`, `"W4.missed"`, `"W9a"`) | `id` + `variant` | Task 26 maps the key to `id` (the part before `.`) and `variant` (the part after it, or a motif name for `motif.missed:*`, or null). |
| internal `CauseHint.kind` | `CandidateCause.kind` | These are identical (`time_control_mix`, `new_opening`, `session_length`, `rating_level`, `protocol_version`). Task 18 produces the M3 names directly. |
| `focus` | not present (M3 computes focuses) | M2 does not emit `focus`. |
| changes without a confirmed onset | `onset*` required | Task 26 emits only transitions that have an onset. |
| lines with `verdict: null` (n < 8) | `verdict` required | Task 26 omits them. |

## Review Focus

1. **Games evaluated from Lichess server evals have no PVs** (`pv: []`), so motif tags and refutation lines would be empty and W1/W3 silently undercounted. Enrichment must fetch PVs for critical plies. Test: Task 22 `refutation PV falls back to engine when cached PV is empty`.
2. **Null-move FENs that are illegal** (side to move in check, a stale en-passant square) would make Stockfish 19 exit. The probe is skipped or the FEN sanitised. Tests: Task 3 `nullMoveFen returns null in check` and `clears en passant`; Task 22 `threat probe skipped when in check`.
3. **Thin data and no benchmark yet** (new account, 0–29 analysed games, reference build not run) must give "insufficient data" / "no benchmark" with have/need counts, never a crash or a ranked weakness from noise. Tests: Task 16 `no reference gives priority 0`, Task 19 `area with too little data reports have and need`, Task 26 `coaching input for an account with 0 analysed games`.
4. **Games without clocks** (daily, correspondence) must leave every W9 metric absent and out of the denominators, not counted as zero. Tests: Task 7 `no clocks: W9 keys absent`, Task 19 `time_trouble_rate counts only clocked games`.
5. **The Maia service is down, or tablebase files are missing.** Enrichment carries on with nulls, records the difficulty model actually used, and never fails the game. Tests: Task 22 `difficulty provider failure yields null difficulty`, Task 20 `missing tablebase files returns null`.

---

### Task 1: chess-core profile contracts and lane barrels

**Files:**
- Create: `packages/chess-core/src/profile/{types.ts,constants.ts,bands.ts,coaching-input.ts,index.ts}`
- Create (each containing exactly `export {};`): `packages/chess-core/src/{board,motifs,features,weakness,stats,timeline,kpi,openings}/index.ts`
- Modify: `packages/chess-core/src/index.ts` (add `export * from "./<folder>/index.js";` for the nine folders)
- Test: `packages/chess-core/test/profile/contracts.test.ts`

**Interfaces:**
- Consumes: M1 `EngineScore`, `Color`, `Phase`, `TimeClass`, `Judgement`, `MoveRecord`, `PositionEval`.
- Produces (`types.ts`):
  - `type Band = "B0"|"B1"|"B2"|"B3"|"B4"|"B5"`; `type Area = "openings"|"middlegame"|"endgame"|"tactics"|"calculation"|"time"|"mindset"`; `AREAS: readonly Area[]` in that order.
  - `type MotifTag = "hangingPiece"|"fork"|"pin"|"skewer"|"discoveredAttack"|"deflection"|"sacrifice"|"promotion"|"mate"|"mateIn1"|"mateIn2"|"mateIn3"|"mateIn4Plus"|"matingNet"|"backRankMate"|"smotheredMate"`
  - `type WeaknessKey = "W1"|"W2"|"W3.shallow"|"W3.deep"|"W4.missed"|"W4.allowed"|"W5"|"W6"|"W7.traps"|"W7.prep"|"W8.basics"|"W8.advanced"|"W9a"|"W9b"|"W9c"|"W9d"|"W10"|"W11"|"W12"`; `type BinomialKey = Exclude<WeaknessKey, "W9d"|"W12">`
  - `type EndgameType = "pawn"|"rook"|"minor"|"queen"|"mixed"`
  - `type SeriesKey = WeaknessKey | \`motif.missed:${MotifTag}\` | \`motif.allowed:${MotifTag}\` | \`opening:${string}\` | \`endgame:${EndgameType}\``
  - `type WeaknessState = "not_an_issue"|"emerging"|"active"|"improving"|"resolved"|"relapsed"`
  - `type Wdl = -1 | 0 | 1`
  - `interface Swing { gain: number; depth: number | null; mateIn: number | null }`; `interface LineAnalysis extends Swing { tags: MotifTag[] }`
  - `interface MultipvLine { uci: string; score: EngineScore /* mover POV */; pv: string[] }`
  - `interface RawMoveFeatures { ply: number; multipv: MultipvLine[] | null; bestPv: string[]; refutationPv: string[]; threat: { nullFen: string; pv: string[]; score: EngineScore /* opponent POV */ } | null; tbWdlBefore: Wdl | null; tbWdlAfter: Wdl | null /* both mover POV */; difficulty: { pBest: number; pOk: number; model: string } | null }`
  - `interface MoveFeatures { isCritical: boolean | null; multipvGap: number | null; playable: number | null; inTop2: boolean | null; best: LineAnalysis | null; refutation: LineAnalysis | null; threat: LineAnalysis | null; threatExisted: boolean | null; threatAddressed: boolean | null }`
  - `interface EnrichedMove extends MoveRecord { fenAfter: string; scoreBefore: EngineScore; scoreAfter: EngineScore /* mover POV */; raw: RawMoveFeatures | null; f: MoveFeatures }`
  - `interface GameContext { gameId: string; platform: "lichess"|"chesscom"; playedAt: Date; timeClass: TimeClass; baseSeconds: number|null; incrementSeconds: number|null; rated: boolean; playerColor: Color; playerRating: number|null; opponentRating: number|null; result: "1-0"|"0-1"|"1/2-1/2"|"*"; score: 1|0.5|0|null; termination: string|null; accuracy: number|null; protocol: string }`
  - `interface AnalysedGame { ctx: GameContext; moves: EnrichedMove[] /* both colours, in ply order */; bookExitPly: number|null; openingLineKey: string|null }`
  - `interface BinomialObs { events: number; opportunities: number; epl: number; exemplarPlies: number[] }`
  - `interface GameObservation { gameId: string; binomial: Partial<Record<BinomialKey, BinomialObs>>; w9dRho: number|null; series: { seriesKey: SeriesKey; events: number; opportunities: number; epl: number }[] }`
  - `interface GameMetrics` (version 1). Fields: `score, expectedScore, playerRating, opponentRating, accuracy, accuracyByPhase: Record<Phase, number|null>, moves, blunders, mistakes, inaccuracies, oneMoveBlunders, lowClock: {moves; decisive}, highClock: {moves; decisive}, conversion: {episode: boolean; converted: boolean|null; dropPly: number|null}, resilience: {episode: boolean; saved: boolean|null}, missedPunish: {opportunities; misses}, tactics: {opportunities; missed; allowed}, missedByMotif: Partial<Record<MotifTag, {opportunities; misses}>>, sharp: {positions; errors}, quiet: {moves; loss}, evalTrend15to35: number|null, timeTrouble: boolean|null, lostOnTime: boolean, timePerMove: Record<Phase, number|null>, criticalTimeRatio: number|null, timeInOpening: number|null, opening: {lineKey; bookExitPly; winAtOpeningEnd; outWorse: boolean|null}, endgame: {type: EndgameType|null; entryWin: number|null; outcome: "converted"|"held"|"failed_conversion"|"failed_hold"|null; tbPositions; tbErrors}, resignedSaveable: boolean|null, weakness: Partial<Record<BinomialKey, BinomialObs>>, w9dRho: number|null, strengths: { kind: StrengthKind; ply: number }[]`
  - `type StrengthKind = "only_move_found"|"tactic_found"|"conversion"|"defence"`
  - `interface BandReferenceStat { metric: MetricId; median: number; p25: number; p40: number; p75: number; pooledRate: number|null; eplPerEvent: number|null; oppsPerGame: number|null; oppSlopePer100: number; nPlayers: number; nGames: number; lowSample: boolean }`; `type BandReference = ReadonlyMap<MetricId, BandReferenceStat>`
  - `type MetricId = BaseMetricId | \`motif_miss_rate:${MotifTag}\` | \`weakness:${WeaknessKey}\` | \`area:${Area}\``, where `BaseMetricId` is the union of the `metric` column of Task 19's catalogue table (write all 36 literals here).
  - `interface TablebaseProbe { readonly maxPieces: number; probe(fen: string): Promise<Wdl | null> /* side-to-move POV */ }`
  - `interface DifficultyInput { fen: string; eloSelf: number|null; eloOppo: number|null; lines: { uci: string; win: number }[] | null; bestUci: string|null }`; `interface DifficultyProvider { readonly model: string; estimate(inputs: DifficultyInput[]): Promise<({ pBest: number; pOk: number } | null)[]> }`
- Produces (`constants.ts`): `N0: Record<WeaknessKey, number>` = W1 300, W2 100, W3.* 60, W4.* 40, W5 30, W6 30, W7.* 40, W8.* 80, W9a 30, W9b 40, W9c 60, W9d 20, W10 600, W11 300, W12 60. `MIN_EVIDENCE` = W1/W2/W11 1000, W3.* 40, W5/W6 15, W8.* 50, W9a 20, W10 1500, and `N0[key]` for every other key. `TRAINABILITY: Record<WeaknessKey, Record<Band, number>>` is copied verbatim from improvement-engine §6.4 (W3.shallow/deep, W7.traps/prep and W8.basics/advanced use their own rows; W4.* use the W4 row; W9a–d the W9 row; W12 is all 0). `RANKABLE: ReadonlySet<WeaknessKey>` is every key except W9d and W12. `WEAKNESS_AREA: Record<WeaknessKey, Area>` maps W1, W2, W4.allowed and W11 to calculation; W3.* and W4.missed to tactics; W5, W6 and W10 to middlegame; W7.* to openings; W8.* to endgame; W9* to time; W12 to mindset. `WEAKNESS_LABEL: Record<WeaknessKey, string>` holds the English labels from §6.2. `EPL_PRECEDENCE = ["W4.missed","W4.allowed","W2","W1","W3.shallow","W3.deep","W8.basics","W8.advanced","W7.traps","W7.prep","W10"] as const`. `CHESSCOM_TO_LICHESS_OFFSET: Record<TimeClass, number>` = bullet 250, blitz 250, rapid 350, classical 300, daily 300 (provisional, ADR 0036).
- Produces (`bands.ts`): `bandOf(lichessEqRating: number): Band`; `toLichessEquivalent(platform, timeClass, rating: number|null): number|null`; `ratingTarget(rating: number|null): number|null` (+200, §16.5).
- Produces (`coaching-input.ts`): `CoachingInput`, `ExemplarPosition`, `Metric`, `RankedWeakness`, `OpeningVerdict`, `AreaScore`, `TimelineChange`, `CandidateCause`, `Basis`, as plain TS interfaces with M3 A1's exact field names and types (see "Contract mapping to M3"), plus:
  - `StrengthEntry = { area: Area; label: string; kind: StrengthKind | "low_weakness"; weaknessId: string|null; metricKey: MetricId|null; bandPercentile: number|null; n: number; exemplars: ExemplarPosition[] /* 1..3 */ }`
  - `CoachingInput.strengths: StrengthEntry[]`

- [ ] **Step 1: Write failing tests**

```ts
expect([899, 900, 1299, 1300, 1700, 1999, 2000, 2299, 2300].map(bandOf))
  .toEqual(["B0","B1","B1","B2","B3","B3","B4","B4","B5"]);
expect(toLichessEquivalent("lichess", "blitz", 1500)).toBe(1500);
expect(toLichessEquivalent("chesscom", "rapid", 1000)).toBe(1350);
expect(toLichessEquivalent("chesscom", "rapid", null)).toBeNull();
expect(ratingTarget(1450)).toBe(1650);
expect(TRAINABILITY.W1.B0).toBe(1.0); expect(TRAINABILITY.W10.B0).toBe(0);
expect(TRAINABILITY["W7.prep"].B5).toBe(1.0); expect(TRAINABILITY["W3.deep"].B0).toBe(0.2);
expect(TRAINABILITY.W12.B3).toBe(0);
expect(N0.W1).toBe(300); expect(MIN_EVIDENCE.W1).toBe(1000); expect(MIN_EVIDENCE["W4.missed"]).toBe(40);
expect(RANKABLE.has("W12")).toBe(false); expect(RANKABLE.has("W9d")).toBe(false);
expect(AREAS).toHaveLength(7);
expectTypeOf(sampleInput).toMatchTypeOf<CoachingInput>(); // a literal fixture with strengths
```

- [ ] **Step 2:** Run `pnpm --filter @ci/chess-core test profile`. Expected: FAIL (modules missing).
- [ ] **Step 3:** Implement the types, constants and helpers. Write the empty barrels and the root re-exports.
- [ ] **Step 4:** Run `pnpm --filter @ci/chess-core test && pnpm --filter @ci/chess-core typecheck && pnpm --filter @ci/chess-core lint`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): profile engine contracts, constants and coaching input type`.

### Task 2: M2 database schema, migration and test utilities

**Files:**
- Create: `packages/db/src/profile-schema.ts`, `packages/db/src/test-utils.ts`, the generated `packages/db/drizzle/00NN_profile_engine.sql`, `packages/db/test/profile-schema.test.ts`
- Modify: `packages/db/src/schema.ts` (games, linkedAccounts, enum), `packages/db/src/index.ts`, `packages/db/drizzle.config.ts` (`schema: ["./src/schema.ts","./src/profile-schema.ts"]`), `packages/db/test/setup.ts`, `apps/worker/test/db.ts`, `.gitignore` (add `/data/`)

**Interfaces:**
- Produces:
  - `analysis_status` gains the value `imported` (a game imported but not selected for analysis). Do not use it as a default in the same migration (Postgres forbids using a value added in the same transaction).
  - `games` gains `plyCount int null`, `analysisTier smallint null` (0–3), `analysisPriority int null`, `selectedAt`, `analysedAt` (timestamptz null) and `enrichmentProtocol text null`. `linkedAccounts` gains `analysisScheduledAt timestamptz null`.
  - Enums: `band` (B0–B5), `weakness_state` (the six states), `state_row_kind` (`transition`, `onset`), `kpi_window` (`last_n`, `week`), `reference_kind` (`band_reference`, `opening_explorer`), `build_status` (`building`, `active`, `superseded`, `failed`), `opening_verdict` (`keep`, `fix`, `learn`, `replace`), `line_kind` (`chosen`, `faced`).
  - Tables (camelCase TS names, snake_case columns):
    - `moveFeatures`: `gameId` FK cascade, `ply`. PK `(gameId, ply)`. Raw columns: `multipv jsonb null`, `bestPv text[]`, `refutationPv text[]`, `threat jsonb null`, `tbWdlBefore smallint null`, `tbWdlAfter smallint null`, `pBest real null`, `pOk real null`, `difficultyModel text null`. Derived columns: `isCritical bool null`, `multipvGap real null`, `inTop2 bool null`, `bestLine jsonb null`, `refutationLine jsonb null`, `threatLine jsonb null`, `threatExisted bool null`, `threatAddressed bool null`. Plus `enrichmentProtocol text`.
    - `positionMultipv` (`fenKey`, `protocol`, `lines jsonb`, `createdAt`; PK `(fenKey, protocol)`) and `positionThreats` (`fenKey`, `protocol`, `scoreCp`, `scoreMate`, `pv text[]`; PK `(fenKey, protocol)`).
    - `positionTb`: `fenKey` PK, `wdl smallint`, `source text` default `'syzygy5'`.
    - `gameMetrics`: `gameId` PK FK cascade, `accountId` FK, `timeClass`, `playedAt`, `protocol`, `enrichmentProtocol`, `difficultyModel`, `metrics jsonb`, `createdAt`. Index `(accountId, timeClass, playedAt)`.
    - `weaknessSeriesPoint`: `accountId`, `timeClass`, `seriesKey text`, `gameId` FK cascade, `playedAt`, `events int`, `opportunities int`, `epl real`, `protocol text`. PK `(accountId, timeClass, seriesKey, gameId)`. Index `(accountId, timeClass, seriesKey, playedAt)`.
    - `weaknessEstimates`: PK `(accountId, timeClass, seriesKey)`. Columns `k`, `n`, `nUnweighted`, `alpha`, `beta`, `mean`, `ciLow`, `ciHigh`, `bandMedian`, `bandP40`, `confidence`, `excess`, `priority`, `percentile`, `state weakness_state`, `pImp real null`, `updatedAt`. Current values, upserted.
    - `weaknessStateHistory`: `id bigserial` PK, `accountId`, `timeClass`, `seriesKey`, `kind state_row_kind`, `fromState null`, `toState`, `atGameId null`, `atPlayedAt null`, `onsetGameId null`, `onsetAt null`, `onsetLow null`, `onsetHigh null`, `pChange real null`, `rateBefore null`, `rateAfter null`, `evidence jsonb`, `causeHints jsonb`, `detectorVersion text`, `protocol text`, `createdAt`. Index `(accountId, timeClass, seriesKey, id)`.
    - `kpiSnapshot`: `id bigserial` PK, `userId uuid null`, `accountId`, `timeClass`, `metric text`, `windowKind`, `windowKey text`, `value real null`, `valueAdjusted real null`, `n int`, `ciLow null`, `ciHigh null`, `protocol text`, `computedAt`. Index `(accountId, timeClass, metric, windowKey, computedAt desc)`. **Append-only.**
    - `referenceBuilds`: `id uuid`, `kind`, `status`, `sourceMonth`, `sourceSha256`, `params jsonb`, `protocol null`, `enrichmentProtocol null`, `difficultyModel null`, `startedAt`, `finishedAt null`, `activatedAt null`, `error null`.
    - `openingExplorer`: `buildId` FK cascade, `fenKey`, `band`, `timeClass`, `uci`, `white int`, `draws int`, `black int`. PK `(buildId, fenKey, band, timeClass, uci)`.
    - `referenceGames`: `id uuid`, `buildId` FK cascade, `lichessGameId`, `pgn`, `timeClass`, `baseSeconds`, `incrementSeconds`, `whiteRating`, `blackRating`, `whiteHash`, `blackHash`, `result`, `termination`, `playedAt`, `status analysis_status`, `error null`. Unique `(buildId, lichessGameId)`.
    - `referenceObservations`: `buildId`, `referenceGameId` FK cascade, `color`, `playerHash`, `band`, `timeClass`, `playerRating`, `opponentRating`, `metrics jsonb`. PK `(referenceGameId, color)`.
    - `bandReference`: `buildId` FK cascade, `band`, `timeClass`, `metric`, `median`, `p25`, `p40`, `p75`, `pooledRate null`, `eplPerEvent null`, `oppsPerGame null`, `oppSlopePer100`, `nPlayers`, `nGames`, `lowSample bool`. PK `(buildId, band, timeClass, metric)`.
    - `gameOpenings`: `gameId` PK FK cascade, `accountId`, `timeClass`, `color`, `fenKeys text[]`, `ucis text[]`, `sans text[]`, `lineKey null`, `lineName null`, `eco null`, `lineKind null`, `lineEntryPly null`, `bookExitPly null`, `playerDeviationPly null`.
    - `repertoireNodes` (PK `(accountId, timeClass, color, fenKey)`; `games int`, `stats jsonb`) and `openingLines` (PK `(accountId, timeClass, color, lineKey)`; `games int`, `verdict opening_verdict null`, `summary jsonb`, `updatedAt`).
  - `truncateAllTables(db: Db): Promise<void>` truncates every table in `public` except Drizzle's migrations table, with `restart identity cascade`. Both test helpers use it instead of their hard-coded table lists.

- [ ] **Step 1: Write failing tests** in `packages/db/test/profile-schema.test.ts`:
  - insert a game with `analysisStatus: "imported"`, then read it back;
  - insert one row into each new table;
  - `truncateAllTables` leaves `select count(*)` at 0 for `band_reference` (which has no FK to games) and for `kpi_snapshot`;
  - inserting two `kpi_snapshot` rows with identical values keeps both (no unique constraint).
- [ ] **Step 2:** Run `docker compose run --rm worker pnpm --filter @ci/db test`. Expected: FAIL.
- [ ] **Step 3:** Write the schema, run `pnpm --filter @ci/db generate`, and check that the SQL contains `ALTER TYPE "public"."analysis_status" ADD VALUE 'imported'`. Add `/data/` to `.gitignore`.
- [ ] **Step 4:** Run the db tests and the whole worker suite in the container. Expected: PASS (existing worker tests are unaffected by the truncate change).
- [ ] **Step 5:** Commit `feat(db): profile engine tables, imported status and truncate helper`.

### Task 3 (P1): Board utilities, material swings and the null move

**Files:**
- Create: `packages/chess-core/src/board/{material.ts,line.ts,null-move.ts,attacks.ts}`, `packages/chess-core/test/board/*.test.ts`, `packages/chess-core/test/fixtures/positions.ts` (the FEN/PV fixtures below, shared by Tasks 3–5)
- Modify: `packages/chess-core/src/board/index.ts`

**Interfaces:**
- Produces:
  - `PIECE_VALUE = { p: 1, n: 3, b: 3, r: 5, q: 9, k: 0 }`, `MATE_GAIN = 100`
  - `materialBalance(fen: string, pov: Color): number`; `pieceCount(fen: string): number` (kings included); `materialSignature(fen: string): string` (stronger side first, e.g. `"KRPvKR"`)
  - `interface LineStep { uci; san; fenBefore; fenAfter; mover: Color; captured: string|null; isCheck: boolean; isMate: boolean; promotion: string|null; from: string; to: string }`
  - `playLine(fen: string, pv: string[], maxPlies?: number): LineStep[]` stops at the first illegal move and never throws.
  - `settledSwing(fen: string, pv: string[], pov: Color, opts?: { maxPlies?: number /* 8 */; settleExtra?: number /* 4 */ }): Swing`
  - `nullMoveFen(fen: string): string | null`
  - `pinnedTo(fen: string, square: string): { pinner: string; behind: string } | null` (the piece on `square` is pinned by an enemy slider to its king or to a higher-value own piece)
  - `isEnPrise(fen: string, square: string): boolean`; `defendersOf(fen, square, color)` and `attackersOf(fen, square, color)` wrap chess.js `attackers`.
- Algorithm (`settledSwing`): `B0 = materialBalance(fen, pov)`. For each `pov` ply `k ≤ maxPlies`, `settled(k)` is the balance after ply k, extended through the **recapture chain** (the next PV moves capture on the destination square of the previous move, at most `settleExtra` plies), minus B0. `depth` is the first k with `settled(k) ≥ 2` or with ply k delivering mate. `gain` is the maximum `settled(k)`, or `MATE_GAIN` if the line mates. `mateIn = (matingPlyIndex + 1) / 2` when the PV ends in checkmate by pov.

- [ ] **Step 1: Write failing tests** with these fixtures (all White to move, `pov: "white"`):

| Name | FEN | PV | Swing |
|---|---|---|---|
| hanging | `4k3/8/8/3q4/8/8/8/3QK3 w - - 0 1` | `d1d5` | `{gain:9, depth:1, mateIn:null}` |
| fork | `r3k3/8/8/1N6/8/8/8/4K3 w - - 0 1` | `b5c7 e8e7 c7a8` | `{gain:5, depth:3, mateIn:null}` |
| pin | `4k3/8/3p4/4n3/8/8/5P2/4R1K1 w - - 0 1` | `f2f4 e8f7 f4e5 d6e5` | `{gain:2, depth:3, mateIn:null}` |
| skewer | `4q3/8/8/4k3/8/8/8/R6K w - - 0 1` | `a1e1 e5d6 e1e8` | `{gain:9, depth:3, mateIn:null}` |
| discovered | `7q/4k3/8/8/3N4/8/1B6/6K1 w - - 0 1` | `d4c6 e7d6 b2h8` | `{gain:9, depth:3, mateIn:null}` |
| deflection | `1r4k1/1q3ppp/8/3Q4/8/8/5PPP/4R1K1 w - - 0 1` | `e1e8 b8e8 d5b7` | `{gain:4, depth:3, mateIn:null}` |
| promotion | `8/4P3/8/8/8/k7/8/K7 w - - 0 1` | `e7e8q` | `{gain:8, depth:1, mateIn:null}` |
| backRank | `6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1` | `d1d8` | `{gain:100, depth:1, mateIn:1}` |
| mateIn2 | `r5k1/5ppp/8/8/8/8/3R1PPP/3R2K1 w - - 0 1` | `d2d8 a8d8 d1d8` | `{gain:100, depth:3, mateIn:2}` |
| smothered | `6rk/6pp/8/6N1/8/8/8/6K1 w - - 0 1` | `g5f7` | `{gain:100, depth:1, mateIn:1}` |
| quiet | start position | `e2e4 e7e5` | `{gain:0, depth:null, mateIn:null}` |

  Plus:
  - `playLine(start, ["e2e4","e2e4"])` has length 1;
  - `materialSignature("8/4k3/8/4K3/4P3/8/8/8 w - - 0 1") === "KPvK"`;
  - `pieceCount` of that FEN is 3;
  - `nullMoveFen("4k3/8/8/r7/8/8/8/R3K3 b - - 0 1") === "4k3/8/8/r7/8/8/8/R3K3 w - - 0 1"`;
  - `nullMoveFen returns null in check`: `"4k3/8/8/8/8/8/8/R3K2r w - - 0 1"` gives null;
  - `clears en passant`: `"rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3"` gives `"... b KQkq - 0 3"`;
  - `pinnedTo` on the pin fixture after `f2f4` at `e5` gives `{ pinner: "e1", behind: "e8" }`.
- [ ] **Step 2:** Run `pnpm --filter @ci/chess-core test board`. Expected: FAIL.
- [ ] **Step 3:** Implement on chess.js. `nullMoveFen` flips the side to move, clears en passant, keeps the clocks, returns null if the side to move is in check, and returns null unless chess.js `validateFen` accepts the result.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): material swings, line replay and null-move fen`.

### Task 4 (P1): Motif tagger (clean-room)

**Files:**
- Create: `packages/chess-core/src/motifs/{tag-line.ts,rules.ts}`, `packages/chess-core/test/motifs/tag-line.test.ts`, `docs/decisions/0031-motif-tagger-clean-room.md`
- Modify: `packages/chess-core/src/motifs/index.ts`

**Interfaces:**
- Consumes: Task 3 `playLine`, `settledSwing`, `pinnedTo`, `isEnPrise`, `defendersOf`, fixtures.
- Produces: `tagLine(fen: string, pv: string[], pov: Color, opts?: { maxPlies?: number }): LineAnalysis`.
- Rules (our own wording; tags are produced only when `swing.depth !== null`, and only from plies ≤ `depth + 1`):
  - `hangingPiece`: pov ply 1 captures a non-pawn piece, and no legal recapture on that square exists afterwards.
  - `fork`: after a pov move, the moved piece attacks ≥ 2 enemy pieces, each the king, worth more than the mover, or undefended. The mover is not en prise. One of the targets is captured later in the line.
  - `pin`: the piece captured at the swing was pinned (`pinnedTo`) by a pov slider in some earlier position of the line with the opponent to move.
  - `skewer`: a pov slider captures X along a line through a square vacated on the previous ply by a more valuable enemy piece.
  - `discoveredAttack`: a pov move vacates a square, and a different pov slider then attacks through it a king (check) or a piece it captures two plies later.
  - `deflection`: a pov capture or check at ply k forces opponent piece D to move at k+1 (to square t). At k+2 pov captures on a square x ≠ t that D defended before k+1.
  - `sacrifice`: `min settled(k) ≤ −2` (relative to B0), the line still reaches `gain ≥ 2` or mate, and the opponent does not promote.
  - `promotion`: a pov move promotes within the swing depth.
  - `mate`, `mateIn1`…`mateIn3`, `mateIn4Plus` come from `mateIn`. `matingNet` means `mateIn ≥ 2`.
  - `backRankMate`: the mating piece is a rook or queen on the mated king's back rank, and the king's forward escape squares hold its own pieces.
  - `smotheredMate`: a knight mates and every square adjacent to the king holds the king's own pieces.

- [ ] **Step 1: Write failing tests** on the Task 3 fixtures:

```ts
expect(tag("hanging").tags).toEqual(["hangingPiece"]);
expect(tag("fork").tags).toContain("fork"); expect(tag("fork").tags).not.toContain("hangingPiece");
expect(tag("pin").tags).toContain("pin"); expect(tag("pin").tags).not.toContain("skewer");
expect(tag("skewer").tags).toContain("skewer"); expect(tag("skewer").tags).not.toContain("pin");
expect(tag("discovered").tags).toContain("discoveredAttack");
expect(tag("deflection").tags).toEqual(expect.arrayContaining(["deflection", "sacrifice"]));
expect(tag("promotion").tags).toContain("promotion");
expect(tag("backRank").tags).toEqual(expect.arrayContaining(["mate", "mateIn1", "backRankMate"]));
expect(tag("mateIn2").tags).toEqual(expect.arrayContaining(["mate", "mateIn2", "matingNet", "backRankMate", "sacrifice"]));
expect(tag("mateIn2").tags).not.toContain("deflection");
expect(tag("smothered").tags).toEqual(expect.arrayContaining(["mateIn1", "smotheredMate"]));
expect(tag("smothered").tags).not.toContain("backRankMate");
expect(tag("quiet").tags).toEqual([]);
expect(tagLine(fixtures.fork.fen, fixtures.fork.pv, "black").tags).toEqual([]); // wrong pov: no swing
```

- [ ] **Step 2:** Run `pnpm --filter @ci/chess-core test motifs`. Expected: FAIL.
- [ ] **Step 3:** Implement each rule as its own function in `rules.ts`. Return tags in `MotifTag` declaration order, without duplicates.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Write ADR 0031. It covers: our own rules written from the published description in improvement-engine §4.4, no code taken from `cook.py` (AGPL), the tag list, and "no swing, no tags". Commit `feat(chess-core): clean-room motif tagger`.

### Task 5 (P1): Per-move features and enriched-move assembly

**Files:**
- Create: `packages/chess-core/src/features/{derive.ts,assemble.ts}`, `packages/chess-core/test/features/*.test.ts`
- Modify: `packages/chess-core/src/features/index.ts`

**Interfaces:**
- Consumes: Tasks 3–4; M1 `winPercent`, `flipScore`, `MoveRecord`, `PositionEval`.
- Produces:
  - `deriveMoveFeatures(move: Pick<MoveRecord,"color"|"fenBefore"|"uci"> & { fenAfter: string }, raw: RawMoveFeatures | null): MoveFeatures`
  - `assembleEnrichedMoves(records: MoveRecord[], evals: PositionEval[], raw: (RawMoveFeatures | null)[], terminal: ParsedGame["terminal"]): EnrichedMove[]`. `raw[i]` belongs to `records[i]`. Scores are converted to mover POV, and a final checkmate ply gets `scoreAfter = { mate: 1 }` as in `buildMoveRecords`.
- Rules:
  - `multipvGap = W(lines[0]) − W(lines[1])` (needs ≥ 2 lines); `isCritical = gap ≥ 15`. With no MultiPV, `isCritical`, `multipvGap`, `playable` and `inTop2` are all null.
  - `playable` = the number of lines within 5 W% of the best. `inTop2` = the played uci is one of the first two lines.
  - `best = tagLine(fenBefore, raw.bestPv, mover)`; `refutation = tagLine(fenAfter, raw.refutationPv, opponent)`; `threat = tagLine(raw.threat.nullFen, raw.threat.pv, opponent)`.
  - `threatExisted = threat.gain ≥ 2 && threat.depth ≤ 4` (mate counts).
  - `threatAddressed = !(threatExisted && refutation.gain ≥ 2 && refutation.depth ≤ 4 && (raw.refutationPv[0] === raw.threat.pv[0] || firstCaptureSquare(refutation) === firstCaptureSquare(threat)))`.

- [ ] **Step 1: Write failing tests:**
  - The threat fixture is `fen = "4k3/8/8/r7/8/8/8/R3K3 b - - 0 1"` with `threat = { nullFen: nullMoveFen(fen), pv: ["a1a5"], score: {cp: 500} }`.
    - Played `e8e7` with `refutationPv ["a1a5"]` gives `threatExisted: true, threatAddressed: false`.
    - Played `a5a1` with `refutationPv ["e1d2"]` gives `threatExisted: true, threatAddressed: true`.
  - MultiPV lines with mover-POV cp `[300, 20, 10]` give `isCritical: true`, `playable: 1`; cp `[30, 25, 20]` give `isCritical: false`, `playable: 3`; played uci equal to `lines[1].uci` gives `inTop2: true`.
  - `raw: null` gives `isCritical: null`, `best: null`, `threatExisted: null`.
  - `assembleEnrichedMoves` on M1's Scholar's mate fixture (`records` from `buildMoveRecords`, `raw` all null):
    - `moves[6].scoreAfter` equals `{ mate: 1 }`;
    - `moves[1].scoreBefore` is the black-POV flip of `evals[1].score`;
    - `moves[0].fenAfter === moves[1].fenBefore`.
- [ ] **Step 2:** Run `pnpm --filter @ci/chess-core test features`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): per-move tactical features and enriched moves`.

### Task 6 (P1): Episodes, time trouble, endgame type and sessions

**Files:**
- Create: `packages/chess-core/src/weakness/{episodes.ts,time-trouble.ts,endgame.ts,sessions.ts}`, tests in `packages/chess-core/test/weakness/`
- Modify: `packages/chess-core/src/weakness/index.ts`

**Interfaces:**
- Consumes: Task 3 `materialBalance`; M1 `MoveRecord`, `Color`.
- Produces:
  - `interface Episode { kind: "winning"|"losing"; startPly: number; endPly: number; outcome: "won"|"drawn"|"lost"; dropPly: number|null }`
  - `extractEpisodes(moves: Pick<MoveRecord,"ply"|"color"|"winBefore"|"winAfter">[], ctx: { playerColor: Color; score: number|null }, opts?: { tWin?: 85; tLose?: 15; minMoves?: 3; minPly?: 21 }): { winning: Episode|null; losing: Episode|null }`. The winning episode is the first run of ≥ 3 consecutive own moves with `winBefore ≥ tWin` at ply ≥ 21. `dropPly` is the first own ply from the start with `winAfter < 65`. The losing episode is ≥ 3 consecutive own moves with `winBefore ≤ tLose` (no ply floor). Outcome comes from `score`.
  - `detectTimeTrouble(moves, ctx: { playerColor; baseSeconds: number|null }): { inTrouble: boolean|null; firstPly: number|null }`. The player is in trouble when some own move has `clockBefore < 0.1·base` and (move number < 40 or ≥ 10 own moves remain). The result is null when there are no clocks or no base.
  - `endgameType(fen: string): EndgameType`. No non-pawn pieces → pawn; only rooks → rook; only minor pieces → minor; only queens → queen; anything else → mixed.
  - `BASIC_SIGNATURES = new Set(["KPvK","KQvK","KRvK","KRvKP","KRPvKR","KQvKP","KPvKP"])`
  - `expectedScore(own: number, opp: number): number = 1 / (1 + 10^((opp − own)/400))`
  - `interface SessionInfo { gameId; sessionId; indexInSession /* 1-based */; afterLoss: boolean; residual: number|null }`
  - `buildSessions(games: { gameId; playedAt: Date; estDurationSeconds: number; score: number|null; expectedScore: number|null }[], gapMinutes = 15): SessionInfo[]`. Games are sorted by `playedAt`. A new session starts when `playedAt − (prev.playedAt + prev.estDuration) ≥ gapMinutes`. `estDuration = base + 40·inc`.
  - `isTimeLoss(ctx: GameContext): boolean` is true for Lichess `"outoftime"`/`"timeout"`, or chess.com `/on time/i`, with the player not winning. `isResignation(ctx)` is true for Lichess `"resign"` or chess.com `/resignation/i`.

- [ ] **Step 1: Write failing tests:**
  - White own moves at plies 21, 23, 25, 27 with winBefore `[90, 88, 91, 70]` and ply 27 winAfter 60, score 0.5 → `winning: { startPly:21, outcome:"drawn", dropPly:27 }`.
  - The same run starting at ply 15 → `winning: null`.
  - winBefore `[10, 12, 9]` and score 0.5 → `losing.outcome === "drawn"`.
  - base 300 with an own move at ply 61 and clockBefore 25 → `inTrouble: true, firstPly: 61`; clockBefore 31 → `false`; clockBefore 25 at ply 89 with 3 own moves left → `false`; no clocks → `{ inTrouble: null }`.
  - `endgameType("8/5k2/8/8/3R4/8/2r5/4K3 w - - 0 1") === "rook"`.
  - Games at 10:00, 10:04 and 11:00 (3+0, so estDuration 180 s) → sessions `[1,1,2]`, `indexInSession` `[1,2,1]`.
  - `expectedScore(1500, 1500) === 0.5`.
  - `isTimeLoss` is true for chess.com termination `"Hikaru won on time"` when the player is black and the result is 1-0.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): episodes, time trouble, endgame types and sessions`.

### Task 7 (P1): Weakness detectors W1–W12

**Files:**
- Create: `packages/chess-core/src/weakness/{detectors.ts,observe-game.ts,tilt.ts}`, `packages/chess-core/test/weakness/{detectors.test.ts,make-game.ts}`, `docs/decisions/0037-weakness-metric-rulings.md`
- Modify: `packages/chess-core/src/weakness/index.ts`

**Interfaces:**
- Consumes: Tasks 1, 5, 6; Task 15's `spearman` is **not** used. W9d uses a local rank-correlation helper in `detectors.ts`, so this task does not wait for Lane P3.
- Produces:
  - `observeGame(game: AnalysedGame, opts?: { tWin?: number }): GameObservation`
  - `tiltResidualDifference(sessions: SessionInfo[]): { tilted: number[]; other: number[] }` (W12 input. "Tilted" = `indexInSession ≥ 4` or `afterLoss`.)
- Rules (P = the player's moves; E = eligible P; D = decisive; ΔW/100 = EPL):

| Key | Opportunities | Event |
|---|---|---|
| W1 | E | D ∧ refutation.gain ≥ 2 ∧ refutation.depth ≤ 3 ∧ refutation.mateIn = null |
| W2 | E ∧ threatExisted | D ∧ threatAddressed = false |
| W3.shallow / W3.deep | E ∧ best.gain ≥ 2 (or mate) ∧ best.depth ≤ 3 / ≥ 4 | ΔW ≥ 15 ∧ inTop2 ≠ true |
| W4.missed | P with scoreBefore mate ∈ [1,4] (decided allowed) | scoreAfter is not a positive mate |
| W4.allowed | E | scoreBefore not a negative mate ∧ scoreAfter negative mate |
| W5 | 1 if winning episode | not won; epl = 1 − score |
| W6 | 1 if losing episode | lost; epl = 0.5 |
| W7.traps | 1 per game | any D at ply ≤ 20 |
| W7.prep | 1 if winAtOpeningEnd known | winAtOpeningEnd ≤ 40 |
| W8.basics / W8.advanced | P with tbWdlBefore ≠ null, split by `BASIC_SIGNATURES` | tbWdlAfter < tbWdlBefore |
| W9a | 1 if clocked and TC ≠ daily | inTrouble; epl = Σ ΔW/100 after firstPly |
| W9b | E with clockBefore < 30, TC ∈ bullet/blitz/rapid | D |
| W9c | E with isCritical = true and timeSpent ≠ null | timeSpent < 0.5 × median own timeSpent ∧ judgement ∈ {mistake, blunder} |
| W9d | (continuous) | Spearman ρ of timeSpent vs `multipvGap ?? 0` over ≥ 10 own clocked moves, else null |
| W10 | E in middlegame, isCritical ≠ true, best.depth = null, scoreBefore cp ∈ [−150, 150] | ΔW ≥ 5 |
| W11 | E with difficulty.pOk ≥ 0.8 | D |

  EPL is attributed once per move, by `EPL_PRECEDENCE`. Keys outside that list keep the ΔW of their own events. `exemplarPlies` lists event plies, by ΔW descending. Series:
  - every binomial key;
  - `motif.missed:<t>` (opportunities: W3 opportunities with t ∈ best.tags; events: W3 events among them);
  - `motif.allowed:<t>` (opportunities: E; events: D with t ∈ refutation.tags);
  - `endgame:<type>` (opportunity: reached the endgame with entry W ≥ 30; event: entry ≥ 70 and not won, or entry in [30, 70) and lost);
  - `opening:<lineKey>` (opportunity 1; event: lost).

- [ ] **Step 1: Write failing tests** using `makeGame()`, which builds an `AnalysedGame` of 30 eligible player moves with ΔW 0, all features neutral, blitz 300+0 with clocks:
  - one move with ΔW 20 and `refutation {gain:3, depth:1, mateIn:null}` → `W1 = {events:1, opportunities:30, epl:0.2}`;
  - the same move also `threatExisted: true, threatAddressed: false` → `W2.epl === 0.2`, `W1.epl === 0`, `W1.events === 1`;
  - a decided move (winBefore 95, ΔW 20) and opponent moves are not counted in W1 opportunities;
  - best `{gain:3, depth:3}` with ΔW 16 → `W3.shallow {events:1, opportunities:1}`; depth 5 → `W3.deep`; `inTop2: true` → 0 events;
  - scoreBefore `{mate:2}`, scoreAfter `{cp:400}`, winBefore 97 → `W4.missed {events:1, opportunities:1}`;
  - scoreBefore `{cp:50}`, scoreAfter `{mate:-3}` → `W4.allowed.events === 1`;
  - tbWdl 1 → 0 on fen `"8/8/4k3/8/4P3/8/4K3/7r w - - 0 1"` (signature KRvKP) → `W8.basics.events === 1`;
  - clockBefore 20 with ΔW 16 → `W9b {events:1}`; clockBefore 20 with ΔW 3 → opportunity only;
  - timeSpent `[1..10]` with multipvGap `[1..10]` → `w9dRho === 1`;
  - quiet middlegame ΔW 6 → `W10.events === 1`; the same with `isCritical: true` → not an opportunity;
  - pOk 0.9 with ΔW 20 → `W11.events === 1`; pOk 0.7 → not an opportunity;
  - a winning episode drawn → `W5 {events:1, opportunities:1, epl:0.5}`;
  - D at ply 15 → `W7.traps.events === 1`;
  - `series` contains `{ seriesKey: "motif.allowed:fork", events: 1, opportunities: 30 }` when the W1 move's refutation tags include fork;
  - `no clocks: W9 keys absent`: with every clock null, `W9a`, `W9b` and `W9c` are undefined and `w9dRho` is null.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement one function per key in `detectors.ts`, composed in `observeGame`.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Write ADR 0037. It records:
  - the sub-series split;
  - W10, W9b and W9c turned into binomial counts;
  - W9d and W12 continuous and not ranked;
  - depth 4 counted as deep;
  - W6 event cost 0.5;
  - W1's settled-swing stand-in for SEE;
  - tablebase-eligible means ≤ 5 men;
  - fixed `T_win = 85` per band until §4.3 is fitted.

  Commit `feat(chess-core): weakness detectors W1-W12`.

### Task 8 (P1): Per-game metrics

**Files:**
- Create: `packages/chess-core/src/weakness/game-metrics.ts`, `packages/chess-core/test/weakness/game-metrics.test.ts`
- Modify: `packages/chess-core/src/weakness/index.ts`

**Interfaces:**
- Consumes: Tasks 6–7.
- Produces: `computeGameMetrics(game: AnalysedGame, obs: GameObservation, strengths: { kind: StrengthKind; ply: number }[]): GameMetrics`. Notable definitions:
  - `oneMoveBlunders` = D ∧ (refutation.depth = 1, or best.mateIn = 1 and the played move is not a mate).
  - `missedPunish` opportunities = P immediately after an opponent `mistake`/`blunder`; a miss = ΔW ≥ 10.
  - `sharp` = E with isCritical; errors = judgement ≠ null.
  - `evalTrend15to35` = player-POV W after ply 70 minus W after ply 30 (null if the game is shorter).
  - `criticalTimeRatio` = mean timeSpent on critical moves divided by the mean on non-critical moves.
  - `opening.winAtOpeningEnd` = player-POV W after the last opening ply.
  - `resignedSaveable` = the player resigned with final player-POV W ≥ 25 (null if they did not resign).
  - `expectedScore` comes from ratings.

- [ ] **Step 1: Write failing tests** on `makeGame` variants:
  - player judgements `[blunder, mistake, inaccuracy, null…]` → `blunders 1, mistakes 1, inaccuracies 1`;
  - phases opening/middlegame with move accuracies `[100, 80]` and `[60]` → `accuracyByPhase.opening === 90`, `middlegame === 60`, `endgame === null`;
  - an opponent blunder at ply 10 followed by the player's ΔW 12 at ply 11 → `missedPunish {opportunities:1, misses:1}`;
  - ratings 1500 vs 1700 → `expectedScore ≈ 0.2403` (4 decimals);
  - `lostOnTime` is true for Lichess termination `"outoftime"` with the player losing;
  - `weakness.W1` equals `obs.binomial.W1`;
  - `strengths` is passed through.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): per-game metrics`.

### Task 9 (P1): Strength exemplar extractor (controller ruling)

**Files:**
- Create: `packages/chess-core/src/weakness/strengths.ts`, `packages/chess-core/test/weakness/strengths.test.ts`
- Modify: `packages/chess-core/src/weakness/index.ts`

**Interfaces:**
- Consumes: Tasks 5–6.
- Produces: `extractStrengthPlies(game: AnalysedGame): { kind: StrengthKind; ply: number }[]`. Rules (player moves only, eligible):
  - `only_move_found`: isCritical = true ∧ played uci = MultiPV line 1.
  - `tactic_found`: best.gain ≥ 2 (or mate) ∧ played uci = `raw.bestPv[0]`.
  - `conversion`: a won winning episode. Use the first own critical ply inside it, else `startPly`.
  - `defence`: a losing episode with score ≥ 0.5. Use the first own critical ply inside it, else `startPly`.
  - At most 3 per kind per game, ordered by `multipvGap` descending, then ply.

- [ ] **Step 1: Write failing tests:**
  - a critical move where played equals line 1 → `[{kind:"only_move_found", ply}]`;
  - a tactic found by playing `bestPv[0]` with best.gain 3 → `tactic_found`;
  - a won winning episode with no critical plies → `conversion` at `startPly`;
  - a drawn losing episode → `defence`;
  - four critical only-moves → 3 returned;
  - decided positions → none.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): strength exemplar extraction`.

### Task 10 (P2): Lichess database access (download, verify, stream, sample)

**Files:**
- Create: `apps/worker/src/reference/lichess-db/{config.ts,download.ts,pgn-stream.ts,headers.ts,sample.ts,fetch-cli.ts}`, tests `apps/worker/test/reference/lichess-db.test.ts`, fixture `apps/worker/test/fixtures/lichess-db-sample.pgn` (5 games, < 10 KB), `docs/decisions/0034-lichess-db-data-handling.md`

**Interfaces:**
- Produces:
  - `resolveDataDir(env = process.env): string` (`DATA_DIR`, or else `<repo>/data` resolved from `import.meta.url`)
  - `lichessDbFileName(month: string): string` → `lichess_db_standard_rated_${month}.pgn.zst`
  - `class ChecksumMismatchError extends Error`
  - `ensureLichessDump(cfg: { dataDir: string; month: string; baseUrl?: "https://database.lichess.org" }, deps?: { fetch?: typeof fetch }): Promise<{ path: string; sha256: string }>`. It downloads to `<dataDir>/lichess/<file>.part` while streaming a SHA-256, fetches `<baseUrl>/standard/sha256sums.txt`, compares, then renames and writes a `<file>.sha256` sidecar. When the file and a matching sidecar already exist it does nothing. On a mismatch it deletes the `.part` file and throws.
  - `readPgnGames(chunks: AsyncIterable<Uint8Array>): AsyncIterable<string>` (splits on a blank line followed by `[Event `)
  - `openZstPgn(path: string): AsyncIterable<string>` (`zlib.createZstdDecompress()`)
  - `interface DumpGameHeader { site; lichessGameId; white; black; whiteElo: number|null; blackElo: number|null; timeControl; baseSeconds: number|null; incrementSeconds: number|null; result; termination: string|null; utcDateTime: Date; hasEval: boolean; hasClock: boolean }`; `parseDumpHeaders(pgn: string): DumpGameHeader`
  - `interface SampledGame { pgn: string; header: DumpGameHeader; timeClass: TimeClass }`
  - `filterSample(games: AsyncIterable<string>, f: { maxGames?: number; sampleRate?: number /* 0..1 */; seed?: number; requireClock?: boolean; timeClasses?: TimeClass[] }): AsyncIterable<SampledGame>`. It skips `TimeControl "-"`, `Termination` `Abandoned`/`Rules infraction`, and games with a missing Elo. Sampling is deterministic, keeping a game when `fnv1a(site + seed) / 2^32 < sampleRate`.
  - CLI: `pnpm --filter @ci/worker exec tsx --conditions=development src/reference/lichess-db/fetch-cli.ts --month 2026-10`

- [ ] **Step 1: Write failing tests:**
  - The fixture is compressed in-test with `zlib.zstdCompressSync` into a temp file; `openZstPgn` yields 5 strings, each starting with `[Event `.
  - `parseDumpHeaders` maps `TimeControl "180+2"` to base 180, inc 2, and `lichessGameId` from the `Site` URL; `hasClock` and `hasEval` are true for the game with `[%clk` / `[%eval`.
  - `filterSample` with `requireClock` drops the clockless game; `sampleRate: 0` yields none; seed 7 gives the same subset on two runs; `maxGames: 2` stops after 2.
  - `ensureLichessDump` with a fake fetch (body `"abc"`, sums line `<sha256("abc")>  lichess_db_standard_rated_2026-10.pgn.zst`) writes the file and sidecar. A second call makes 0 body fetches. A wrong sum throws `ChecksumMismatchError` and leaves no file and no `.part` behind.
- [ ] **Step 2:** Run `docker compose run --rm worker pnpm --filter @ci/worker test reference/lichess-db`. Expected: FAIL.
- [ ] **Step 3:** Implement with streams only. Never buffer the whole file.
- [ ] **Step 4:** Run the tests. Expected: PASS. Also check `node -e "require('zlib').createZstdDecompress"` in the container. Expected: no error.
- [ ] **Step 5:** Write ADR 0034. It covers:
  - the `data/` directory;
  - SHA-256 against Lichess's `sha256sums.txt`;
  - one month, streamed;
  - disk needs (about 30 GB per recent month, so `--month` can pick an older, smaller month);
  - hashed usernames.

  Commit `feat(worker): lichess database download, verification and streaming sampler`.

### Task 11 (P2): Own rating-band opening explorer

**Files:**
- Create: `apps/worker/src/reference/explorer/{aggregate.ts,build.ts,query.ts}`, `apps/worker/src/jobs/build-explorer.ts`, tests `apps/worker/test/reference/explorer.test.ts`, `docs/decisions/0035-opening-explorer-build.md`

**Interfaces:**
- Consumes: Task 10 `SampledGame`, `ensureLichessDump`, `openZstPgn`, `filterSample`; Task 2 tables; Task 1 `bandOf`.
- Produces:
  - `explorerBand(whiteElo: number, blackElo: number): Band` (band of the mean rating)
  - `aggregateGame(acc: Map<string, [number, number, number]>, game: SampledGame, maxPly: number): void`. The key is `fenKey|band|timeClass|uci`; the value is white/draws/black. Only the first `maxPly` SAN tokens are replayed with chess.js `move()`, never the full PGN.
  - `buildOpeningExplorer(deps: { db: Db; games: AsyncIterable<SampledGame> }, opts: { maxGames: number /* 2_000_000 */; maxPly: number /* 30 */; minRowCount: number /* 3 */; flushEveryGames: number /* 50_000 */; source: { month: string; sha256: string } }): Promise<{ buildId: string; games: number; rows: number }>`.
    1. Create a `building` row in `referenceBuilds`.
    2. Upsert batches with `ON CONFLICT … DO UPDATE SET white = opening_explorer.white + excluded.white` (and the same for draws and black).
    3. Prune rows with a total below `minRowCount`.
    4. In one transaction, mark the build `active` and the previous one `superseded`, and delete the superseded build's rows.
  - `interface ExplorerMove { uci; white; draws; black; total }`
  - `queryExplorer(db, fenKey, band, timeClass): Promise<ExplorerMove[]>` (active build only, sorted by total desc)
  - `loadExplorerIndex(db, fenKeys: string[], band, timeClass): Promise<Map<string, ExplorerMove[]>>` (batched by 1000)
  - `inTheory(index, fenKey, uci): boolean` is true when the position total is ≥ 50 and the move's share is ≥ 5%.
  - `handleBuildExplorer(deps: { db; dataDir }, payload: { month?: string; maxGames?: number }): Promise<{ buildId }>`. `month` defaults to the previous calendar month (UTC).

- [ ] **Step 1: Write failing tests** (DB in Docker), with three B2 blitz games `1.e4 e5`, `1.e4 c5` and `1.d4 d5` (results 1-0, 0-1, ½):
  - With `maxPly 2, minRowCount 1`, the start position has `e2e4 {white:1, draws:0, black:1, total:2}` before `d2d4 {total:1}`.
  - With `minRowCount 2` there is no `d2d4` row.
  - A second build supersedes the first: after it, the first build's row count is 0 and `queryExplorer` reads the new build.
  - `inTheory` is false when the position total is 49.
  - A failed build (throwing iterator) leaves status `failed` and the previous active build still in place.
- [ ] **Step 2:** Run the tests in the container. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Write ADR 0035. Parameters: one month, 2M games, ply ≤ 30, prune < 3, bands by mean rating, 4 TC classes (correspondence excluded), quarterly. Commit `feat(worker): rating-band opening explorer from the lichess database`.

### Task 12 (P2): Opening classification from the CC0 chess-openings dataset

**Files:**
- Create: `packages/chess-core/data/chess-openings/{a,b,c,d,e}.tsv` plus `packages/chess-core/data/chess-openings/SOURCE` (repo URL, commit sha, CC0-1.0), `packages/chess-core/scripts/build-openings.ts`, the generated `packages/chess-core/src/openings/openings.generated.ts`, `packages/chess-core/src/openings/classify.ts`, `packages/chess-core/test/openings/classify.test.ts`
- Modify: `packages/chess-core/src/openings/index.ts`, `biome.json` (ignore `**/*.generated.ts`)

**Interfaces:**
- Produces:
  - `OPENINGS: ReadonlyMap<string /* fenKey */, { eco: string; name: string }>`
  - `classifyOpening(plies: { fenAfter: string; color: Color }[]): { eco; name; fenKey; ply; mover: Color } | null` (the deepest named position reached)
  - `lineFamily(name: string): string` (the text before the first `,`)
  - `lineKey(color: Color, name: string): string` → `${color}:${slug(lineFamily(name))}`
  - `lineKind(entryMover: Color, playerColor: Color): "chosen" | "faced"`

- [ ] **Step 1:** Run `gh api repos/lichess-org/chess-openings/commits/master -q .sha`. Download the five TSVs at that sha and write `SOURCE`.
- [ ] **Step 2: Write failing tests:**
  - `1.e4 c6 2.d4 d5 3.e5` gives eco `"B12"` and a name starting `"Caro-Kann Defense: Advance Variation"`;
  - `1.e4 e5 2.Nf3 Nc6 3.Bc4` gives `"C50"`, `"Italian Game"`;
  - `1.Nf3 d5 2.d4` and `1.d4 d5 2.Nf3` give equal results (a transposition);
  - `lineFamily("Sicilian Defense: Najdorf Variation, English Attack") === "Sicilian Defense: Najdorf Variation"`;
  - `lineKind("black", "white") === "faced"`;
  - the start position gives null.
- [ ] **Step 3:** Run `pnpm --filter @ci/chess-core test openings`. Expected: FAIL.
- [ ] **Step 4:** Write `build-openings.ts`. It replays each TSV `pgn` with chess.js, keys by `fenKey`, keeps the longest name when there are duplicates, and writes the generated file. Run it, then implement `classify.ts`.
- [ ] **Step 5:** Run the tests and lint. Expected: PASS.
- [ ] **Step 6:** Commit `feat(chess-core): opening classification from lichess chess-openings (CC0)`.

### Task 13 (P2): Repertoire tree, struggle detection and verdicts

**Files:**
- Create: `packages/chess-core/src/openings/{repertoire.ts,lines.ts,verdicts.ts}`, `packages/chess-core/test/openings/{repertoire,verdicts}.test.ts`, `apps/worker/src/openings/rebuild-repertoire.ts`, `apps/worker/test/openings/rebuild-repertoire.test.ts`, `docs/decisions/0041-repertoire-and-verdicts.md`

**Interfaces:**
- Consumes: Task 12; Task 11 `loadExplorerIndex`, `inTheory`, `ExplorerMove`; Task 15 `Z80`; Task 6 `expectedScore`.
- Produces:
  - `interface RepertoireGame { gameId; color: Color; plies: { uci; san; fenKeyBefore; fenKeyAfter; mover: Color }[] /* ≤ 30 */; score: number; expectedScore: number|null; line: { key; name; eco; kind: "chosen"|"faced"; entryPly } | null; analysed: { winAtOpeningEnd: number|null; openingAccuracy: number|null; timeInOpening: number|null; deltaW: (number|null)[]; bestUci: (string|null)[] } | null }`
  - `buildRepertoireTree(games: RepertoireGame[], color: Color): Map<string, RepertoireNode>`, with `RepertoireNode = { fenKey; games; wins; draws; losses; score; perfVsExpected: number|null; analysedGames; avgWinAtOpeningEnd: number|null; avgOpeningAccuracy: number|null; avgTimeInOpening: number|null; children: { uci; san; toFenKey; games; score }[]; parents: string[] }`. Nodes are keyed by fenKey, so transpositions merge.
  - `bookExit(plies, index: Map<string, ExplorerMove[]>, playerColor): { bookExitPly: number|null; playerDeviationPly: number|null }`
  - `type StruggleReason = "score_below_expected"|"worse_after_opening"|"repeated_deviation_error"`
  - `summariseLines(games: RepertoireGame[], index, opts: { band; timeClass }): LineSummary[]`, where `LineSummary` carries M3 `OpeningVerdict`'s fields plus `struggles`, `fixSpot` and `choiceFenKey`.
  - `detectStruggles(line, t = STRUGGLE_THRESHOLDS): StruggleReason[]`, with `STRUGGLE_THRESHOLDS = { minGames: 8, perfDrop: -0.10, worseWin: 40, minAnalysed: 5, deviationMinGames: 3, deviationShare: 0.3, deviationDeltaW: 10 }`:
    - score_below: perf ≤ −0.10 and the upper end of the 80% normal interval < 0;
    - worse_after_opening: analysed ≥ 5 and avgWin ≤ 40;
    - repeated_deviation_error: one `(fenKey, uci)` with ΔW ≥ 10 in ≥ 3 games and ≥ 30% of the line's games.
  - `decideVerdict(line, struggles, ctx: { colorGames: number; alternatives: ExplorerMove[] }): { verdict: OpeningVerdict["verdict"] | null; reason: string }`. Rules, in order:
    1. games < 8 → null.
    2. repeated_deviation_error → `fix`.
    3. score_below ∧ games ≥ 15 ∧ perf ≤ −0.15 ∧ kind = chosen → `replace`. Alternatives are the top 3 explorer moves at the choice node with total ≥ 100, excluding the current move, by score for the side to move.
    4. kind = chosen ∧ median player deviation ply ≤ 10 ∧ line share ≥ 10% → `learn`.
    5. any remaining struggle → `learn`.
    6. otherwise `keep`.
  - `rebuildRepertoire(db, accountId, timeClass): Promise<{ lines: number }>`. It fills missing `gameOpenings` rows for imported games (Tier 1 included), loads the explorer index for the player's band, and replaces `repertoireNodes` and `openingLines` for the account × TC in one transaction.

- [ ] **Step 1: Write failing tests** (pure):
  - 10 white Italian games (6 wins, 2 draws, 2 losses, expected 0.5) → the Italian node has `games 10, score 0.7, perfVsExpected 0.2`; verdict `keep`.
  - Transposition: `1.Nf3 d5 2.d4` + `1.d4 d5 2.Nf3` → that node has `games: 2` and two parents.
  - 8 Caro-Kann games where the same `(fenKey, uci)` has ΔW 12 in 3 games → struggles include `repeated_deviation_error`; verdict `fix`; `fixSpot.games === 3`.
  - 15 chosen games, each score 0 and expected 0.2 → `replace`. With explorer moves `[{uci:"g1f3", total:500, white:300, draws:100, black:100}, {uci:"b1c3", total:90,…}]`, alternatives are `[g1f3]` only.
  - The same stats with kind faced → `learn`.
  - 7 games → null.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement the pure parts, then `rebuildRepertoire`. DB test: seed 12 imported games, run it twice, and check that row counts are equal and that `opening_lines` holds the expected line key.
- [ ] **Step 4:** Run `pnpm --filter @ci/chess-core test openings` and the worker test in the container. Expected: PASS.
- [ ] **Step 5:** Write ADR 0041. It covers:
  - FEN-keyed DAG;
  - lines = `lineFamily` of the deepest named position;
  - the theory definition;
  - thresholds;
  - faced lines are never `replace`;
  - playstyle-clash replacement deferred until playstyle axes exist.

  Commit `feat: repertoire tree, struggle detection and opening verdicts`.

### Task 14 (P2): Band reference builder

**Files:**
- Create: `apps/worker/src/reference/band-reference/{select-players.ts,analyse-reference-game.ts,aggregate.ts,finalize.ts,read.ts}`, `apps/worker/src/jobs/build-band-reference.ts`, tests `apps/worker/test/reference/band-reference.test.ts`, `docs/decisions/0036-band-reference-method.md`

**Interfaces:**
- Consumes: Task 10; Task 22 `enrichGame`; Task 5 `assembleEnrichedMoves`; Tasks 7–8 `observeGame`, `computeGameMetrics`; Task 15 `fitBetaBinomial`, `betaQuantile`, `quantile`; Task 19 `KPI_CATALOGUE` (per-player KPI values); M1 `parsePgn`, `evaluateGame`, `buildMoveRecords`.
- Produces:
  - `selectReferencePlayers(scan: () => AsyncIterable<SampledGame>, opts: { playersPerCell: number /* 20 */; gamesPerPlayer: number /* 10 */; salt: string; seed: number }): Promise<SampledGame[]>`. It makes two passes over the capped stream. Pass 1 counts games per `(playerHash, band(rating at game), timeClass)`. Players with ≥ `gamesPerPlayer` games are reservoir-sampled per cell. Pass 2 collects the first `gamesPerPlayer` games of each chosen player.
  - `handleBuildBandReference(deps: { db; dataDir; enqueue(refGameId: string): Promise<void> }, payload: { month?: string; maxScanGames?: number /* 3_000_000 */; playersPerCell?: number; gamesPerPlayer?: number })`. It inserts `referenceGames` (status `pending`) and enqueues one `analyse-reference-game` job per game (priority 50).
  - `handleAnalyseReferenceGame(deps: { db; pool; cache; enrich: EnrichDeps }, payload: { referenceGameId })`. It runs the full per-game pipeline for **both** colours and writes two `referenceObservations` rows. Once no `pending`/`analysing` games remain for the build, it calls `finalizeBandReference`.
  - `aggregateBandReference(sides: { playerHash; band; timeClass; playerRating; opponentRating; metrics: GameMetrics; ctx: GameContext }[]): BandReferenceStat-with-cell rows`:
    - binomial keys: per-player `(k_i, n_i)` → `pooledRate = Σk/Σn`; Beta-binomial fit → `p25/p40/median/p75` from `betaQuantile`; `eplPerEvent = Σepl/Σk`; `oppsPerGame = Σn/games`;
    - continuous KPIs: per-player means → type-7 quantiles;
    - `oppSlopePer100`: the OLS slope of the per-game value on `(opp − own)/100`;
    - `lowSample = nPlayers < 10`.
  - `finalizeBandReference(db, buildId)` writes `bandReference` and activates the build (the same supersede rule as Task 11).
  - `loadBandReference(db, band, timeClass): Promise<{ buildId: string; ref: BandReference } | null>`. A low-sample cell falls back to the nearest band with the same TC whose cell is not low-sample, else null.

- [ ] **Step 1: Write failing tests:**
  - Pure aggregate: 10 players with W1 `(6, 300)` and 10 with `(12, 300)`, each over 10 games → `pooledRate` toBeCloseTo(0.03, 6), `p25 < 0.03 < p75`, `nPlayers 20`, `lowSample false`. With 5 players → `lowSample true`.
  - Slope: per-game value `= 1 + 0.5·(opp−own)/100` exactly → `oppSlopePer100` toBeCloseTo(0.5, 6).
  - DB: 30 generated PGNs from 3 players (with `playersPerCell 2, gamesPerPlayer 5`) → 10 `referenceGames`. Analysing with a fake pool and memory caches writes 20 observations. Finalize writes rows for `weakness:W1` and activates the build. `loadBandReference` returns the build; a second build supersedes it.
  - `reference rows keep the difficulty model`: the build row records the provider's `model`.
- [ ] **Step 2:** Run the tests in the container. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Write ADR 0036. It covers:
  - player-sampled cells (20 players × 10 games per band × TC, configurable);
  - both colours used;
  - Beta-binomial fit instead of raw per-player rates;
  - the opponent slope used for KPI adjustment;
  - references are comparable only within the same protocol, enrichment and difficulty model;
  - provisional chess.com offsets, to be replaced by pooled user data later;
  - the quarterly schedule.

  Commit `feat(worker): band reference builder from the lichess database`.

### Task 15 (P3): Statistics numerics

**Files:**
- Create: `packages/chess-core/src/stats/{rng.ts,special.ts,beta.ts,intervals.ts,quantiles.ts,beta-binomial.ts,correlation.ts,weights.ts}`, `packages/chess-core/test/stats/*.test.ts`
- Modify: `packages/chess-core/src/stats/index.ts`

**Interfaces:**
- Produces:
  - `createRng(seed: number): () => number` (mulberry32); `lgamma(x)`; `regIncBeta(x, a, b)` (continued fraction); `betaQuantile(p, a, b)` (bisection on `regIncBeta`, 1e-10); `sampleBeta(rng, a, b)` (Marsaglia–Tsang gamma ratio)
  - `Z80 = 1.2815515655446004`; `interface Interval { low: number; high: number }`
  - `wilson(k, n, z = Z80): Interval` (n = 0 gives `{0, 1}`); `bootstrapMeanInterval(values: number[], opts?: { resamples?: 1000; seed?: 1; level?: 0.8 }): Interval | null` (null when n < 2)
  - `quantile(sorted: number[], p: number): number` (type 7)
  - `percentileFromQuartiles(value: number, ref: { p25; p40; median; p75 }, higherIsBetter: boolean): number`. This is piecewise linear through (p25, 25), (p40, 40), (median, 50) and (p75, 75). Below p25 it extends the p25–p40 slope; above p75 it extends the median–p75 slope. The result is clamped to [1, 99], and a lower-is-better metric gives `100 − raw`.
  - `fitBetaBinomial(data: { k: number; n: number }[]): { mu: number; phi: number }`. `mu = Σk/Σn`, and `phi` maximises the Beta-binomial log-likelihood over 60 log-spaced values in [1, 1e5].
  - `spearman(x: number[], y: number[]): number | null` (average ranks for ties; null for constant input)
  - `weightedCounts(points: { events; opportunities; ageDays }[], halfLifeDays = 60): { k: number; n: number }`

- [ ] **Step 1: Write failing tests:**

```ts
expect(betaQuantile(0.1, 16, 684)).toBeCloseTo(0.0159747, 6);
expect(betaQuantile(0.9, 16, 684)).toBeCloseTo(0.0303238, 6);
expect(1 - regIncBeta(0.02, 16, 684)).toBeCloseTo(0.672363, 5);
expect(wilson(7, 20).low).toBeCloseTo(0.229497, 5); expect(wilson(7, 20).high).toBeCloseTo(0.493269, 5);
expect(wilson(0, 10)).toEqual({ low: 0, high: expect.closeTo(0.141069, 5) });
expect(wilson(0, 0)).toEqual({ low: 0, high: 1 });
expect(wilson(2, 10).high).toBeCloseTo(0.398404, 5);
const r = createRng(42); const m = mean(Array.from({ length: 100_000 }, () => sampleBeta(r, 2, 5)));
expect(Math.abs(m - 2 / 7)).toBeLessThan(0.005);
expect(take(createRng(1), 5)).toEqual(take(createRng(1), 5));
expect(quantile([1, 2, 3, 4], 0.5)).toBe(2.5); expect(quantile([1, 2, 3, 4], 0.25)).toBe(1.75);
const ref = { p25: 0.01, p40: 0.015, median: 0.02, p75: 0.03 };
expect([0.02, 0.015, 0.03, 0, 0.04].map((v) => percentileFromQuartiles(v, ref, false))).toEqual([50, 60, 25, 99, 1]);
expect(fitBetaBinomial(Array(20).fill({ k: 5, n: 100 }))).toEqual({ mu: 0.05, phi: 1e5 });
const mixed = fitBetaBinomial([...Array(10).fill({ k: 2, n: 100 }), ...Array(10).fill({ k: 8, n: 100 })]);
expect(mixed.mu).toBeCloseTo(0.05, 10); expect(mixed.phi).toBeLessThan(200);
expect(spearman([1, 2, 3], [3, 2, 1])).toBe(-1); expect(spearman([1, 1, 2], [1, 2, 3])).toBeCloseTo(0.866025, 5);
expect(spearman([1, 1, 1], [1, 2, 3])).toBeNull();
expect(weightedCounts([{ events: 1, opportunities: 10, ageDays: 0 }, { events: 2, opportunities: 10, ageDays: 60 }])).toEqual({ k: 2, n: 15 });
expect(bootstrapMeanInterval([5])).toBeNull();
```

- [ ] **Step 2:** Run `pnpm --filter @ci/chess-core test stats`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): seeded statistics numerics`.

### Task 16 (P3): Posteriors, P_imp and priority

**Files:**
- Create: `packages/chess-core/src/stats/{posterior.ts,priority.ts}`, `packages/chess-core/test/stats/{posterior,priority}.test.ts`, `docs/decisions/0038-posterior-priority-conventions.md`
- Modify: `packages/chess-core/src/stats/index.ts`

**Interfaces:**
- Consumes: Task 15; Task 1 constants.
- Produces:
  - `interface BetaPosterior { alpha; beta; mean; interval: Interval }`; `posterior(k, n, prior: { mu: number; n0: number }): BetaPosterior` (`alpha = n0·mu + k`, `beta = n0·(1−mu) + n − k`)
  - `probAbove(p: BetaPosterior, threshold: number): number`
  - `pImprovement(current, baseline, opts?: { deltaRel?: 0.15; draws?: 10_000; seed?: 1 }): number` = P(θc < θb·(1 − δ))
  - `pRegression(current, baseline, opts?: { factor?: 1.15; draws?: 10_000; seed?: 1 }): number`
  - `interface WeaknessEstimate { key: WeaknessKey; k; n; nUnweighted; posterior: BetaPosterior; bandMedian: number|null; bandP40: number|null; percentile: number|null; confidence: number; excess: number; trainability: number; priority: number; precise: boolean; minEvidenceMet: boolean; reason: "ok"|"no_benchmark"|"insufficient_data" }`
  - `estimateWeakness(input: { key; points: { events; opportunities; epl; ageDays }[]; gamesInWindow: number; band: Band; ref: BandReferenceStat | null; fatigue?: number }): WeaknessEstimate`:
    - only points with `ageDays ≤ 183` count, recency-weighted (half-life 60);
    - prior `{ mu: ref.pooledRate, n0: N0[key] }`, or `Beta(1,1)` with `reason "no_benchmark"` and priority 0 when there is no ref;
    - `confidence = probAbove(post, ref.median)`;
    - `excess = max(0, mean − median) × (nUnweighted / gamesInWindow) × ref.eplPerEvent`;
    - `priority = excess × TRAINABILITY[key][band] × confidence × (1 − fatigue)`, with fatigue 0 in M2;
    - `precise = width ≤ 0.25·median`; `minEvidenceMet = nUnweighted ≥ MIN_EVIDENCE[key]`;
    - `reason "insufficient_data"` when there are fewer than 30 games (§6.3), which also forces priority 0.
  - `rankWeaknesses(est: WeaknessEstimate[]): WeaknessEstimate[]` (RANKABLE only, priority desc, then key)

- [ ] **Step 1: Write failing tests:**
  - `estimateWeakness({ key:"W1", points:[{events:10, opportunities:400, epl:2.5, ageDays:0}], gamesInWindow:40, band:"B1", ref:{ pooledRate:0.02, median:0.02, p40:0.017, eplPerEvent:0.25, … } })`:
    - `posterior.alpha 16, beta 684`, `mean ≈ 0.0228571`;
    - interval ≈ `[0.0159747, 0.0303238]`;
    - `confidence ≈ 0.672363`;
    - `excess` toBeCloseTo(0.0071429, 6) (= (0.0228571 − 0.02) × 400/40 opportunities per game × 0.25);
    - `priority ≈ 0.0048026` (= 0.0071429 × 1.0 × 0.672363);
    - `precise false`, `minEvidenceMet false`, `reason "ok"`.
  - With `ref: null` → `reason "no_benchmark"`, `priority 0`. With `gamesInWindow 29` → `reason "insufficient_data"`, `priority 0`.
  - A point with `ageDays 200` is ignored.
  - `pImprovement(post(Beta(12,988)), post(Beta(30,970)))` is within 0.01 of 0.99096. With both `Beta(50,950)` it is within 0.01 of 0.20296. `pRegression(Beta(45,955), Beta(30,970))` is within 0.01 of 0.87883. The same seed gives identical results.
  - `rankWeaknesses` drops W12 and W9d and sorts by priority.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Write ADR 0038. It covers:
  - 80% intervals everywhere;
  - the 6-month window with a 60-day half-life;
  - excess from player opportunities per game × band EPL per event;
  - fatigue is 0 until plans exist (M6/M7);
  - no ranking without a benchmark or 30 games.

  Commit `feat(chess-core): weakness posteriors, P_imp and priority`.

### Task 17 (P3): Change detection (BOCPD, CUSUM, EWMA)

**Files:**
- Create: `packages/chess-core/src/timeline/{bocpd.ts,cusum.ts,ewma.ts}`, `packages/chess-core/test/timeline/detectors.test.ts`
- Modify: `packages/chess-core/src/timeline/index.ts`

**Interfaces:**
- Consumes: Task 15 `lgamma`.
- Produces:
  - `runBocpd(obs: { k: number; n: number }[], opts: { hazard: number /* 1/200 */; prior: { a: number; b: number }; maxRun: number /* 500 */ }): Float64Array[]` gives the run-length posterior after each observation.
  - `changeSummary(post: Float64Array, t: number, opts?: { windowGames?: 50; minPriorGames?: 10; level?: 0.8 }): { pChange: number; mapRun: number; onsetIndex: number; onsetLow: number; onsetHigh: number }`. Here `pChange = Σ_{r < min(windowGames, t − minPriorGames)} post[r]` and `onsetIndex = t − mapRun + 1` (1-based). The low/high bounds come from the smallest highest-mass set of run lengths covering `level`.
  - `cusum(obs, opts: { p0: number; p1: number; h: number /* 4 */ }): { alarmGame: number | null; statistic: number }` (1-based)
  - `ewmaDrift(x: number[], opts: { lambda: 0.1; level: number; sigma: number; k: 2 }): { alarmGame: number | null; z: number }` (z starts at `level`; alarm when `z > level + k·sigma·sqrt(λ/(2−λ))`)
- Algorithm (Adams–MacKay with Beta-binomial predictive):

```
runs = [{ logp: 0, a: prior.a, b: prior.b }]
for each (k, n):
  pred_r = lchoose(n,k) + lbeta(a_r + k, b_r + n − k) − lbeta(a_r, b_r)
  grow_r = logp_r + pred_r + log(1 − H);   cp = logsumexp_r(logp_r + pred_r + log H)
  runs   = [{ cp, prior }] ++ [{ grow_r, a_r + k, b_r + n − k }]   // truncate to maxRun
  normalise; emit exp(logp) by run length
```

- [ ] **Step 1: Write failing tests** with `prior {a:1.5, b:48.5}`, `hazard 1/200`, `maxRun 500`:
  - Up: 60 × `(1,50)` then 40 × `(3,50)` → at t = 100, `pChange ≥ 0.99`, `onsetIndex 61`, `onsetLow ≤ 61 ≤ onsetHigh`, `onsetHigh − onsetLow ≤ 6`.
  - Down (60 × `(3,50)`, 40 × `(1,50)`) → `onsetIndex 61`.
  - Flat (alternating `(1,50)`, `(2,50)`, 100 games) → `pChange < 0.2`.
  - Empty (60 × `(0,0)`) → `pChange < 0.3`.
  - `cusum` up `{p0:0.02, p1:0.03, h:4}` on the up series → `alarmGame 66`. Down `{p0:0.06, p1:0.04}` on the down series → `67`. The flat series → null.
  - `ewmaDrift([0.02, 0.02, 0.06, 0.06], { lambda:0.1, level:0.02, sigma:0.01, k:2 })` → `alarmGame 4`.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement in log space.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): bocpd, cusum and ewma change detectors`.

### Task 18 (P3): Weakness timeline states, onset and cause hints

**Files:**
- Create: `packages/chess-core/src/timeline/{states.ts,series.ts,causes.ts}`, `packages/chess-core/test/timeline/{states,series,causes}.test.ts`, `docs/decisions/0039-timeline-detector-and-states.md`
- Modify: `packages/chess-core/src/timeline/index.ts`

**Interfaces:**
- Consumes: Tasks 16, 17.
- Produces:
  - `DETECTOR_VERSION = "tl-v1"`
  - `interface Signals { confirmedUp; confirmedDown; cusumUp; confidenceAbove: number; atOrBelowMedian; belowP40Streak: number; pImp: number|null; drift: boolean }`
  - `nextState(prev: WeaknessState | null, s: Signals): WeaknessState`
  - `interface SeriesPoint { gameId; playedAt: Date; events; opportunities; protocol: string }`
  - `interface TimelineRow { kind: "transition"|"onset"; from: WeaknessState|null; to: WeaknessState; atGameId; atPlayedAt: Date; onset: { gameId; playedAt: Date; low: Date; high: Date } | null; pChange: number|null; rateBefore: number|null; rateAfter: number|null; evidence: { k; n; confidence } }`
  - `evaluateSeries(points: SeriesPoint[], ctx: { ref: { median; p40; pooledRate } | null; n0: number; minEvidence: number }, opts?: { hazard?: 1/200; windowGames?: 50 }): { state: WeaknessState; rows: TimelineRow[]; pImp: number|null }`. It replays the series in order.
    - The BOCPD prior is `Beta(1.5, 48.5)` scaled to the pooled rate (`a = 50·mu`, `b = 50·(1−mu)`).
    - A change is confirmed when `pChange ≥ 0.8`, `|rateAfter/rateBefore − 1| ≥ 0.15`, and opportunities since the onset ≥ `minEvidence`. A confirmation appends an `onset` row.
    - CUSUM runs on the current segment with `p0` = the segment rate and `p1 = 1.5·p0` (after ≥ 10 games), and resets after each confirmation.
    - `confidenceAbove` uses the posterior of the current segment with prior `{ mu: pooledRate, n0 }`.
    - Drift uses EWMA from the moment `resolved` is entered.
    - `pImp` compares the segments either side of the last confirmed change.
    - Without a ref the series stays `not_an_issue`.
  - `causeHints(before: CauseGame[], after: CauseGame[]): CandidateCause[]`, with `CauseGame = { tc: string /* "180+0" */; opponentRating: number|null; lineKey: string|null; playedAt: Date; protocol: string }`. Kinds and triggers:
    - `time_control_mix`: the dominant TC changes and its share shifts ≥ 30 pp;
    - `new_opening`: a line < 10% before and ≥ 30% after;
    - `session_length`: games per active day up ≥ 50%;
    - `rating_level`: mean opponent rating shifts ≥ 75;
    - `protocol_version`: the protocol sets differ.

    `description` is deterministic English, for example `"Most games moved from 300+0 to 180+0"`.
- State rules (`nextState`, first match wins):

```
if prev in {resolved}:            return drift ? "relapsed" : "resolved"
if prev in {active,emerging,relapsed,improving} && belowP40Streak >= 10: return "resolved"
if prev in {active,emerging,relapsed} && (confirmedDown || (pImp ?? 0) >= 0.85): return "improving"
if prev == "improving" && !confirmedUp: return "improving"
if confidenceAbove >= 0.7:        return "active"
if confirmedUp || cusumUp:        return "emerging"
if prev in {active,emerging,relapsed} && !atOrBelowMedian: return prev
return "not_an_issue"
```

- [ ] **Step 1: Write failing tests:**
  - `nextState` table cases:
    - `(resolved, drift)` → relapsed;
    - `(active, belowP40Streak 10)` → resolved;
    - `(active, confirmedDown)` → improving;
    - `(null, confidenceAbove 0.75)` → active;
    - `(not_an_issue, cusumUp)` → emerging;
    - `(null, all false)` → not_an_issue.
  - `evaluateSeries` with ref `{median 0.02, p40 0.017, pooledRate 0.02}`, n0 300, minEvidence 1000:
    - 100 × `(1,50)` → rows `[{from:null, to:"not_an_issue"}]`, final `not_an_issue`.
    - 60 × `(1,50)` + 60 × `(3,50)` → final `active`; an `onset` row with `onset.gameId === points[60].gameId` (± 2 games).
    - The up series followed by 80 games alternating `(0,50)` and `(1,50)` (rate 0.01) → states contain `improving` before `resolved`, final `resolved`.
    - Then 30 × `(3,50)` → `relapsed` appears after `resolved`.
    - `ref: null` → only `not_an_issue`.
  - `causeHints`:
    - 20 × `300+0` before, then 15 × `180+0` + 5 × `300+0` after → includes `{kind:"time_control_mix", before:"300+0", after:"180+0"}`;
    - mean opponent rating 1500 → 1600 → `rating_level`;
    - a line at 0% then 40% → `new_opening`;
    - equal inputs → `[]`.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Write ADR 0039. It covers:
  - hazard 1/200, window 50, minimum 10 prior games;
  - confirmation rule;
  - CUSUM 1.5× as an early warning;
  - onset rows appended when a state was entered before confirmation;
  - W9d and W12 (continuous) not run through BOCPD in M2;
  - full recompute per build, which is deterministic.

  Commit `feat(chess-core): weakness timeline states, onsets and cause hints`.

### Task 19 (P3): KPI catalogue, snapshots and area scores

**Files:**
- Create: `packages/chess-core/src/kpi/{catalogue.ts,snapshots.ts,area-scores.ts,iso-week.ts}`, `packages/chess-core/test/kpi/*.test.ts`, `docs/decisions/0040-kpi-catalogue-and-area-scores.md`
- Modify: `packages/chess-core/src/kpi/index.ts`

**Interfaces:**
- Consumes: Task 15; Task 1 `GameMetrics`, `BandReference`; Task 6 `SessionInfo`, `isTimeLoss`.
- Produces:
  - `interface KpiGame { ctx: GameContext; metrics: GameMetrics; session: SessionInfo | null }`
  - `interface KpiDefinition { metric: MetricId; area: Area | null; kind: "mean"|"rate"|"last"; unit: Metric["unit"]; higherIsBetter: boolean | null; minN: number; label: string; extract(g: KpiGame): number | { k: number; n: number } | null }`
  - `KPI_CATALOGUE` holds the rows below, plus generated rows `motif_miss_rate:<tag>` (rate, from `missedByMotif`, minN 20) and `weakness:<key>` (rate, from `weakness[key]`, `minN = N0[key]` opportunities):

| metric | kind/interval | area | better | minN |
|---|---|---|---|---|
| rating | last / none | null | ↑ | 1 |
| perf_vs_expected | mean / bootstrap | null | ↑ | 20 |
| accuracy, accuracy.opening, accuracy.middlegame, accuracy.endgame | mean / bootstrap | null, openings, middlegame, endgame | ↑ | 20 (endgame 20 endgames) |
| blunders_per_game, one_move_blunders_per_game | mean | calculation | ↓ | 20 |
| blunder_rate_low_clock | rate (lowClock) | time | ↓ | 40 moves |
| conversion_rate / resilience_rate | rate (episodes) | middlegame | ↑ | 15 episodes |
| missed_punish_rate | rate | tactics | ↓ | 30 |
| tactics_missed_per_game, tactics_allowed_per_game | mean | tactics | ↓ | 20 |
| sharp_error_rate | rate | calculation | ↓ | 30 |
| quiet_loss_per_move | mean of quiet.loss/quiet.moves | middlegame | ↓ | 20 |
| eval_trend_15_35 | mean | middlegame | ↑ | 20 |
| time_trouble_rate, losses_on_time_rate | rate (clocked games only) | time | ↓ | 20 |
| time_per_move.opening/.middlegame/.endgame | mean | time | null | 20 |
| critical_time_ratio | mean | time | ↑ | 20 |
| eval_after_opening | mean (winAtOpeningEnd) | openings | ↑ | 20 |
| opening_out_worse_rate | rate | openings | ↓ | 20 |
| book_exit_ply | mean | openings | ↑ | 20 |
| time_in_opening | mean | openings | null | 20 |
| endgame_conversion_rate / endgame_hold_rate | rate | endgame | ↑ | 15 |
| tb_errors_per_endgame | mean (tbPositions > 0) | endgame | ↓ | 20 |
| score_after_loss / score_after_win | mean (session.afterLoss) | mindset | ↑ | 20 |
| session_accuracy_decline | mean accuracy(index ≥ 4) − mean(index 1) | mindset | ↑ | 20 |
| first_game_accuracy / later_games_accuracy | mean | mindset | ↑ | 20 |
| saveable_resignation_rate | rate | mindset | ↓ | 10 |

  - `isoWeekKey(d: Date): string` (UTC ISO week, `"2026-W47"`)
  - `interface KpiSnapshotRow { metric; windowKind: "last_n"|"week"; windowKey: string; value: number|null; valueAdjusted: number|null; n: number; ciLow: number|null; ciHigh: number|null }`
  - `computeKpis(games: KpiGame[], window: { kind: "last_n"; n: 50 } | { kind: "week"; isoWeek: string }, ref: BandReference | null): KpiSnapshotRow[]`. `last_n` uses the n most recent games by `playedAt`; `windowKey` is `"last50"` or the ISO week. `valueAdjusted = value − ref[metric].oppSlopePer100 × mean((opp − own)/100)`, and the interval is shifted by the same amount (null for `last`).
  - `compareKpi(a, b, def): "improved"|"worse"|"no_clear_change"`. The intervals must separate, or for rates a two-proportion z-test must give |z| ≥ 1.96. Direction comes from `higherIsBetter`.
  - `AREA_COMPONENTS: Record<Area, MetricId[]>`:
    - openings: accuracy.opening, eval_after_opening, weakness:W7.traps, weakness:W7.prep;
    - middlegame: accuracy.middlegame, weakness:W10, weakness:W5, weakness:W6;
    - endgame: accuracy.endgame, weakness:W8.basics, weakness:W8.advanced;
    - tactics: weakness:W3.shallow, weakness:W3.deep, weakness:W4.missed, missed_punish_rate;
    - calculation: weakness:W1, weakness:W2, weakness:W11, weakness:W4.allowed, blunders_per_game;
    - time: weakness:W9a, weakness:W9b, weakness:W9c, critical_time_ratio;
    - mindset: score_after_loss, session_accuracy_decline.
  - `computeAreaScores(kpis: KpiSnapshotRow[], ref: BandReference | null): AreaScore[]` (M3 shape `{ area, score, n, sufficient, needMore }`):
    - a component counts when `n ≥ minN` and a ref stat exists;
    - score = mean of `percentileFromQuartiles`;
    - `sufficient` = at least half the components count;
    - otherwise `score null` and `needMore = minN − n` of the first component;
    - mindset has no benchmark in M2, so `score null, sufficient false, needMore null`.

- [ ] **Step 1: Write failing tests:**
  - `isoWeekKey(2026-11-21) === "2026-W47"`, `isoWeekKey(2027-01-01) === "2026-W53"`, `isoWeekKey(2026-01-01) === "2026-W01"`;
  - blunders over 3 games `[0, 2, 1]` → `value 1, n 3`, interval contains 1;
  - `time_trouble_rate counts only clocked games`: 10 clocked games (2 in trouble) plus 5 daily → `n 10`, `ciHigh ≈ 0.398404`;
  - 60 games with `last_n` → `n 50`, using the most recent;
  - a week window includes only that week's games;
  - `oppSlopePer100 2` with mean (opp − own) = +100 → `valueAdjusted = value − 2`;
  - `compareKpi` on rates 30/100 → 10/100 (lower better) → `improved`; 12/100 vs 10/100 → `no_clear_change`;
  - `area with too little data reports have and need`: endgame with accuracy.endgame n 12 (minN 20) and no other components → `{ score: null, sufficient: false, needMore: 8 }`;
  - an area whose components all sit at the band median → `score 50`.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Write ADR 0040. It covers:
  - the catalogue;
  - last-50 and ISO-week windows;
  - opponent adjustment by the reference slope;
  - snapshot rows appended only when value or n changed;
  - area-score method;
  - mindset without a benchmark.

  Commit `feat(chess-core): kpi catalogue, snapshots and area scores`.

### Task 20 (P4): Local 5-piece Syzygy via Stockfish SyzygyPath

**Files:**
- Create: `apps/worker/src/tablebase/{fetch-syzygy.ts,syzygy-probe.ts,cached-probe.ts}`, `apps/worker/src/tablebase/syzygy-345.sha256` (manifest), `apps/worker/test/tablebase/*.test.ts`, `docs/decisions/0032-local-syzygy-probe.md`
- Modify: `apps/worker/src/engine/engine-pool.ts` (`PoolOptions.syzygyPath?: string`, passed to `UciEngine.start`), `docker-compose.yml` (worker env `SYZYGY_PATH: /app/data/syzygy`, `DATA_DIR: /app/data`), `.env.example`

**Interfaces:**
- Consumes: M1 `EnginePool`, `AnalysisLine`; Task 1 `TablebaseProbe`, `Wdl`; Task 3 `pieceCount`; Task 2 `positionTb`.
- Produces:
  - `wdlFromLines(lines: AnalysisLine[]): Wdl | null`: `mate > 0` or `cp ≥ 19000` → 1; `mate ≤ 0` or `cp ≤ −19000` → −1; any other cp → 0; no lines → null. (Stockfish reports a tablebase win as `cp 20000 − plies`.)
  - `createSyzygyProbe(opts: { stockfishPath: string; syzygyPath: string; size?: 1; nodes?: 1000 }): Promise<TablebaseProbe & { close(): Promise<void> }>` (`maxPieces 5`). A position with > 5 pieces returns null without an engine call. When the directory has no `.rtbw` files the probe returns null and logs one warning.
  - `createCachedTbProbe(probe: TablebaseProbe, db: Db): TablebaseProbe` (reads and writes `positionTb`)
  - `fetchSyzygy(opts: { dataDir: string; baseUrl?: "https://tablebase.lichess.ovh/tables/standard/3-4-5"; manifestPath: string; fetch?: typeof fetch }): Promise<{ files: number; bytes: number }>` (verifies every file against the manifest; skips files that are already present and verified)

- [ ] **Step 1:** Check the expected results against the Lichess tablebase API: `curl -s "https://tablebase.lichess.org/standard?fen=8/4k3/8/4K3/4P3/8/8/8_w_-_-_0_1" | jq .category` gives `"draw"`; the same position with `_b_` gives `"loss"`; `8/8/8/8/8/4k3/8/KQ6_w_-_-_0_1` gives `"win"`. List the mirror (`curl -s <baseUrl>/`). If the mirror publishes checksums, build the manifest from them; otherwise download once and record SHA-256s (trust on first use, noted in the ADR). Commit the manifest only.
- [ ] **Step 2: Write failing tests:**
  - `wdlFromLines` on `cp 19980` → 1, `cp -19990` → −1, `cp 35` → 0, `mate 3` → 1, `mate 0` → −1, `[]` → null;
  - a 6-piece FEN makes 0 engine calls (fake pool);
  - `missing tablebase files returns null` (empty temp dir);
  - `fetchSyzygy` with a fake fetch and a 2-file manifest writes both files, a corrupted body throws `ChecksumMismatchError`, and a re-run makes 0 fetches;
  - Docker integration (real binary and `SYZYGY_PATH`, failing if `KPvK.rtbw`/`KQvK.rtbw` are missing when `IN_DOCKER=1`): the three FENs from Step 1 give `0`, `-1`, `1`;
  - the cached probe makes the second lookup without the engine;
  - `main pool has no SyzygyPath`: `EnginePool.create` without `syzygyPath` sends no `setoption name SyzygyPath` (spy on the spawned engine's stdin through a fake binary script).
- [ ] **Step 3:** Run `docker compose run --rm worker pnpm --filter @ci/worker test tablebase`. Expected: FAIL.
- [ ] **Step 4:** Implement. Run `pnpm --filter @ci/worker exec tsx --conditions=development src/tablebase/fetch-syzygy.ts` in the container once (about 1 GB into `data/syzygy`).
- [ ] **Step 5:** Run the tests. Expected: PASS.
- [ ] **Step 6:** Write ADR 0032. It covers:
  - a separate pool, so `sf19-n300k-v1` evals are unchanged;
  - tablebase scores parsed from `cp ±20000`;
  - 5 pieces locally;
  - 6–7 pieces only through the API in drills (M6).

  Commit `feat(worker): local syzygy tablebase probe`.

### Task 21 (P4): Maia licence check, difficulty providers and `services/maia`

**Files:**
- Create: `apps/worker/src/difficulty/{spread.ts,maia-client.ts,select.ts}`, `apps/worker/test/difficulty/*.test.ts`, `docs/decisions/0033-maia-licence-and-difficulty.md`
- Create (branch A only): `services/maia/{pyproject.toml,Dockerfile,app/__init__.py,app/main.py,app/model.py,tests/test_api.py}`; Modify `docker-compose.yml` (service `maia` under profile `maia`; worker env `MAIA_URL: ${MAIA_URL:-}`), `.env.example`

**Interfaces:**
- Consumes: Task 1 `DifficultyProvider`, `DifficultyInput`.
- Produces:
  - `createSpreadDifficulty(): DifficultyProvider` (`model "spread-v1"`). It returns null without ≥ 2 lines. `playable` = the number of lines with `win ≥ best − 5`; `pOk = playable / lines.length`; `pBest = 1 / playable`.
  - `createMaiaDifficulty(opts: { url: string; fetch?: typeof fetch; timeoutMs?: 10_000; batchSize?: 64 }): Promise<DifficultyProvider>`. The model id is read from `GET /healthz`. It calls `POST /v1/predict {items:[{fen, eloSelf, eloOppo}]}` and gets `{items:[{probs: Record<uci, number>} | {error: "invalid_fen"}]}`. `pBest = probs[bestUci] ?? 0`; `pOk = Σ probs[m]` over the known lines with `win ≥ best − 5`. Network or 5xx errors throw `MaiaUnavailableError`.
  - `selectDifficultyProvider(env: { MAIA_URL?: string }): Promise<DifficultyProvider>`: Maia when the URL is set and `/healthz` answers, otherwise spread.

- [ ] **Step 1: Licence check (do this first).** Run `gh api repos/CSSLab/maia2 --jq '.license.spdx_id'` and `curl -s https://pypi.org/pypi/maia2/json | jq '.info.license'`. Read the README for the **weights'** licence and where they come from, and check the licences of the runtime dependencies (`python-chess` is GPL-3.0; running it as a separate, undistributed server process is acceptable, the same as Stockfish).
  - Rule: **compatible** only if both the code and the weights grant commercial use (MIT/BSD/Apache-2.0, or GPL-family run unmodified as a separate server process).
  - **Incompatible** if anything is non-commercial (e.g. CC BY-NC), unlicensed (all rights reserved), or AGPL (pending owner approval).
  - Record the evidence and the outcome in ADR 0033. Compatible → **branch A** (Steps 2–7). Incompatible → **branch B**: do Steps 2–4 and 7 for the spread provider only, leave `MAIA_URL` unset, and do not create `services/maia`.
- [ ] **Step 2: Write failing tests** (TS):
  - spread: lines W `[60, 58, 40]` → `{ pOk: 2/3, pBest: 0.5 }`; `[60, 30, 20]` → `{ pOk: 1/3, pBest: 1 }`; one line → null.
  - Maia client with a fake fetch: `probs {e2e4:0.5, d2d4:0.3, g1f3:0.2}`, lines `[{e2e4,55},{d2d4,52},{g1f3,40}]`, best `e2e4` → `{ pBest: 0.5, pOk: 0.8 }`. 130 inputs are sent in 3 batches. A 503 throws `MaiaUnavailableError`.
  - `selectDifficultyProvider({})` → `model "spread-v1"`.
- [ ] **Step 3:** Run `pnpm --filter @ci/worker test difficulty`. Expected: FAIL.
- [ ] **Step 4:** Implement the TS side. Run the tests. Expected: PASS.
- [ ] **Step 5 (A):** Write `services/maia`. It is a FastAPI app with the model loaded once at startup (maia2, CPU, the `rapid` or `blitz` model chosen by env `MAIA_MODEL`). Elo values are clamped to the model's supported range. At most 256 items per request; an invalid FEN gives a per-item `{error:"invalid_fen"}`. `GET /healthz` returns `{model:"maia2-<type>-<pkg version>"}`. `pytest` covers the API with an injected fake model (sum of probs = 1, invalid FEN, 257 items → 422). The Dockerfile is `python:3.12-slim` with pinned versions, and the weights are downloaded at build time into the image (never into the repo).
- [ ] **Step 6 (A):** Run `docker compose --profile maia up -d --build maia`, then `docker compose run --rm maia pytest`, then `curl -s localhost:<port>/healthz` from the worker container. Expected: tests PASS and the model id is printed. Real-model smoke: the start position's top move is one of `e2e4, d2d4, g1f3, c2c4`.
- [ ] **Step 7:** Commit `feat(worker): difficulty providers with maia licence gate` (plus `feat(maia): maia-2 difficulty service` in branch A).

### Task 22 (P5): Enrichment pass (MultiPV, PV completion, threat probe, TB, difficulty)

**Files:**
- Create: `apps/worker/src/enrichment/{critical.ts,store.ts,enrich-game.ts,persist.ts}`, `apps/worker/test/enrichment/*.test.ts`, `docs/decisions/0030-enrichment-protocol.md`
- Modify: `apps/worker/src/engine/protocol.ts`, `apps/worker/src/jobs/analyse-game.ts` (optional `enrich` deps; when present, `move_features` are replaced in the same transaction as `moves`, and `games.enrichmentProtocol` and `analysedAt` are set)

**Interfaces:**
- Consumes: M1 `EnginePool`, `PositionEval`, `MoveRecord`, `ParsedGame`; Task 3 `nullMoveFen`, `pieceCount`; Task 1 `RawMoveFeatures`, `TablebaseProbe`, `DifficultyProvider`; Task 2 tables. It does not depend on Tasks 20 and 21, only on their interfaces.
- Produces:
  - In `protocol.ts`: `MULTIPV_PROTOCOL = "sf19-n300k-mpv3-v1"`, `THREAT_PROTOCOL = "sf19-n50k-null-v1"`, `ENRICHMENT_PROTOCOL = "enr-v1"`, `THREAT_NODES = 50_000`, `MULTIPV = 3`, `MAX_CRITICAL_PLIES = 30`
  - `selectCriticalPlies(records: MoveRecord[], colors: Set<Color>, max = 30): number[]`. A ply is selected when `deltaW ≥ 5` or the previous ply's `deltaW ≥ 10`. Over `max`, the highest ΔW are kept (ties: lower ply). Returned ascending.
  - `interface EnrichmentStore { getMultipv(keys: string[]): Promise<Map<string, MultipvLine[]>>; putMultipv(e: { fenKey; lines }[]): Promise<void>; getThreat(keys: string[]): Promise<Map<string, { score; pv }>>; putThreat(...): Promise<void> }`; `createDbEnrichmentStore(db)`; `createMemoryEnrichmentStore()`
  - `interface EnrichDeps { pool: Pick<EnginePool,"analyse">; store: EnrichmentStore; tb?: TablebaseProbe; difficulty?: DifficultyProvider; log?: (msg: string) => void }`
  - `enrichGame(deps: EnrichDeps, input: { parsed: ParsedGame; evals: PositionEval[]; records: MoveRecord[]; colors: Set<Color>; elo: { white: number|null; black: number|null } }): Promise<{ raw: (RawMoveFeatures | null)[]; difficultyModel: string | null }>`. `raw[i]` is null for colours not requested.
    - MultiPV 3 at `ENGINE_NODES` on `fenBefore` of critical plies.
    - `refutationPv` = `evals[i+1].pv`, or for critical plies with an empty PV, a MultiPV probe of `fenAfter` (first line).
    - Threat probe at `THREAT_NODES` on `nullMoveFen(fenBefore)` for eligible plies of requested colours, skipped when null.
    - TB: when `pieceCount ≤ 5`, `tbWdlBefore = probe(fenBefore)` and `tbWdlAfter = −probe(fenAfter)`.
    - Difficulty is batched for requested eligible plies. Lines = MultiPV lines plus the played move (`winAfter`). A provider error gives null difficulty and one log line.
    - Never writes `position_evals`.
  - `persistMoveFeatures(tx, gameId: string, records: MoveRecord[], raw, derived?: MoveFeatures[]): Promise<void>` (replace)

- [ ] **Step 1: Write failing tests** (fake pool recording `(fen, nodes, multipv)`, memory stores):
  - `selectCriticalPlies` with ΔW `{3:6, 4:0, 5:12, 6:2, 7:1}` → `[3,5,6]`; with `max 2` → `[3,5]`;
  - MultiPV calls are exactly the critical player plies' `fenBefore` with `{ nodes: 300_000, multipv: 3 }`; a second run makes 0 MultiPV calls (cache);
  - the threat probe is called with the side flipped and `{ nodes: 50_000, multipv: 1 }`;
  - `threat probe skipped when in check`; it is also skipped for decided plies and for opponent plies;
  - `refutation PV falls back to engine when cached PV is empty` (Lichess-style `pv: []` evals);
  - a fake TB probe is called only for ≤ 5-piece positions, and `tbWdlAfter` is negated;
  - `difficulty provider failure yields null difficulty`: a throwing provider still resolves, every difficulty is null and `difficultyModel` is null;
  - comparability: every pool call from `evaluateGame` in the analyse job still uses `multipv: 1`, and `position_evals` row count is unchanged by enrichment;
  - analyse-game DB test: with `enrich`, `move_features` holds one row per player ply, `games.enrichmentProtocol === "enr-v1"`, and a re-run leaves the same row count; without `enrich` the behaviour is unchanged (existing tests pass).
- [ ] **Step 2:** Run `docker compose run --rm worker pnpm --filter @ci/worker test enrichment jobs`. Expected: FAIL.
- [ ] **Step 3:** Implement. Validate every FEN with chess.js before sending it to the pool.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Write ADR 0030. It covers:
  - the enrichment layer and its ids;
  - why MultiPV results never replace the MultiPV-1 eval;
  - critical-ply selection and its known bias (critical positions found without a prior swing are not probed; W9c and is_critical are comparable only with references built the same way);
  - cost (about +25–40% CPU);
  - bumping `ENRICHMENT_PROTOCOL` re-enriches from the cache without new main evals.

  Commit `feat(worker): enrichment pass with multipv, threat probe and tablebase`.

### Task 23 (P5): Analysis scheduler for large histories

**Files:**
- Create: `apps/worker/src/scheduler/{select.ts,precision.ts,schedule-analysis.ts}`, `apps/worker/test/scheduler/*.test.ts`, `docs/decisions/0042-analysis-scheduler.md`
- Modify:
  - `apps/worker/src/jobs/import-account.ts`: rows get `analysisStatus: "imported"` and `plyCount` (counted from SAN tokens). There is no per-game enqueue; after the run it enqueues `schedule-analysis`.
  - `apps/worker/src/queue/queues.ts`: `enqueueAnalyse(gameId, opts?: { priority?: number })`, plus queue `"schedule-analysis"` with payload `{ accountId; plan?: "free"|"pro" }` and jobId `schedule-<accountId>`, `removeOnComplete: true`, `removeOnFail: true`.
  - `apps/worker/src/jobs/reconcile.ts`: pass `analysisPriority` when re-enqueueing.
  - The existing import tests: expect `imported` and 1 schedule job.
  - If `apps/worker/src/cli.ts` exists, `runAnalyse` runs the scheduler after the import.

**Interfaces:**
- Consumes: Task 2 columns; M1 12b `importCursor` and the reconciler.
- Produces:
  - `TIER_LIMITS = { free: { importMax: 1000, p0: 50, p1: 0, p2: false, p3PerWeek: 20 }, pro: { importMax: 10_000, p0: 200, p1: 100, p2: true, p3PerWeek: 300 } }`, `P2_PER_MONTH = 15`, `P2_MAX_MONTHS = 36`, `P1_MIN_RECENT = 20`, `RECENT_DAYS = 90`, `MIN_PLIES = 10`, `BATCH_SIZE = 25`
  - `interface CandidateGame { id; timeClass; playedAt: Date; createdAt: Date; rated: boolean; plyCount: number|null; hasLichessEvals: boolean; status; tier: number|null }`
  - `primaryTimeClass(games, now): TimeClass | null` (most games in the last 90 days; ties go to the longer TC); `activeTimeClasses(games, now): TimeClass[]`
  - `selectForAnalysis(input: { games: CandidateGame[]; plan; now; firstScheduledAt: Date; p3SelectedLast7d: number; stopped: Set<TimeClass>; inFlightBackfill: Map<TimeClass, number> }): { gameId: string; tier: 0|1|2|3; priority: number }[]`. Rules (§8c):
    - candidates are `imported`, with plyCount ≥ 10 or null;
    - bullet only when it is the primary TC;
    - rated games before casual;
    - P0 = most recent in primary up to `p0` (counting already-selected);
    - P3 = created after `firstScheduledAt` in active TCs, up to `p3PerWeek − p3SelectedLast7d`;
    - P1 (pro) = most recent per other active TC up to 100;
    - P2 (pro) = up to 15 per calendar month per TC within 36 months, evenly spaced, newest months first;
    - P1/P2 add at most `BATCH_SIZE` per TC per run, only when `inFlightBackfill` for that TC is 0 and the TC is not stopped.
  - `bullmqPriority(tier, plan, hasLichessEvals) = BASE·2 + (hasLichessEvals ? 0 : 1)`, with BASE t0 {pro 1, free 2}, t3 {pro 3, free 4}, t1 5, t2 10
  - `isPrecise(estimates: { key: WeaknessKey; ciLow; ciHigh; bandMedian: number|null; nUnweighted }[]): boolean`. Every rankable estimate must have `width ≤ 0.25·median` or `nUnweighted ≥ MIN_EVIDENCE[key]`. An empty list is false.
  - `handleScheduleAnalysis(deps: { db; queues; now?: () => Date }, payload): Promise<{ selected: number; byTier: Record<0|1|2|3, number> }>`. It sets `pending`, `analysisTier`, `analysisPriority` and `selectedAt` in one transaction, enqueues afterwards, and sets `analysisScheduledAt` on the first run. The import job is given `max = TIER_LIMITS[plan].importMax`.

- [ ] **Step 1: Write failing tests** (pure):
  - free, 70 rated blitz (primary) and 30 recent rapid → 50 tier-0, all blitz, most recent first;
  - pro, 250 blitz, 30 rapid recent, 40 bullet recent → 200 tier-0 blitz, 25 tier-1 rapid, 0 bullet;
  - plyCount 8 is skipped; casual games come only after rated ones run out;
  - pro P2 with 40 months × 20 blitz (P0 took the newest 200) → 25 tier-2 from month 11, newest first; months older than 36 are never picked;
  - P3 free with 30 games after `firstScheduledAt`: `p3SelectedLast7d 0` → 20, and 15 → 5;
  - a stopped TC → no tier 1/2; an in-flight backfill → no new tier 1/2;
  - `bullmqPriority(0,"pro",true) === 2`, `(0,"free",false) === 5`, `(2,"pro",false) === 21`;
  - `isPrecise`: width 0.004 with median 0.02 → true; width 0.01, n 500, key W1 → false; n 1000 → true; `[]` → false.
- [ ] **Step 2:** DB tests:
  - import leaves `imported` rows and one `schedule-analysis` job;
  - `handleScheduleAnalysis` sets `pending` with tier and priority, and `queue.getJob("analyse-<id>").opts.priority` matches;
  - a re-run selects 0;
  - the reconciler re-enqueues a lost pending game with its priority.
- [ ] **Step 3:** Run `docker compose run --rm worker pnpm --filter @ci/worker test scheduler jobs recovery`. Expected: FAIL.
- [ ] **Step 4:** Implement.
- [ ] **Step 5:** Run the tests. Expected: PASS (including the updated M1 tests).
- [ ] **Step 6:** Write ADR 0042. It covers:
  - the `imported` status;
  - the priority table;
  - batch-wise adaptive stopping;
  - `plan` comes from the payload until M7 entitlements;
  - the P3 window is a rolling 7 days.

  Commit `feat(worker): tiered analysis scheduler with adaptive stopping`.

### Task 24 (I): Analysis-time features, game metrics and series rows

**Files:**
- Create: `apps/worker/src/profile/{analyse-features.ts,load-analysed-games.ts,requeue-unenriched.ts}`, `apps/worker/test/profile/analyse-features.test.ts`
- Modify: `apps/worker/src/jobs/analyse-game.ts`, `apps/worker/src/enrichment/persist.ts`

**Interfaces:**
- Consumes: Tasks 5, 6, 7, 8, 9, 11 (`loadExplorerIndex`, `inTheory`), 12, 13 (`bookExit`), 22.
- Produces:
  - `computeGameFeatures(input: { ctx: GameContext; parsed; evals; records; raw; explorer: Map<string, ExplorerMove[]> | null }): { enriched: EnrichedMove[]; derived: MoveFeatures[]; metrics: GameMetrics; observation: GameObservation; bookExitPly: number|null; lineKey: string|null }`. Phases are recomputed with `detectPhases(fens, bookExitPly ?? 20)`. Strengths come from `extractStrengthPlies`.
  - The analyse job writes, in the moves transaction: the derived `move_features` columns, the `game_metrics` row, and replaces the game's `weakness_series_point` rows (each binomial key plus each series entry). After commit it calls `deps.onAnalysed?.(accountId, timeClass)`.
  - `loadAnalysedGames(db, accountId, timeClass): Promise<{ ctx: GameContext; metrics: GameMetrics; protocol: string }[]>`; `loadSeriesPoints(db, accountId, timeClass): Promise<Map<SeriesKey, SeriesPoint[]>>`; `loadExemplarMoves(db, refs: { gameId; ply }[]): Promise<Map<string, { move: MoveRecord-row; features: move_features-row; game: games-row }>>`
  - `requeueUnenriched(db, queues): Promise<number>` sets `pending` (tier 2) on `done` games whose `enrichmentProtocol` ≠ `ENRICHMENT_PROTOCOL` and enqueues them. Main evals come from the cache, so only enrichment costs CPU.

- [ ] **Step 1: Write failing tests** (DB, fake pool, memory stores, spread difficulty):
  - analysing M1's Scholar's mate fixture for an account playing black writes 1 `game_metrics` row with `metrics.version === 1` and `metrics.blunders === 1`;
  - `weakness_series_point` has a `W1` row with `opportunities` equal to black's eligible moves;
  - a re-run keeps the same row counts;
  - `requeueUnenriched` picks up a `done` game with null `enrichmentProtocol` and leaves a game at `"enr-v1"` alone;
  - `loadAnalysedGames` returns contexts with `score` from the player's POV.
- [ ] **Step 2:** Run the tests in the container. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(worker): per-game features, metrics and weakness series at analysis time`.

### Task 25 (I): `build-profile` job, wiring and events

**Files:**
- Create: `apps/worker/src/jobs/build-profile.ts`, `apps/worker/src/profile/{estimates.ts,timeline.ts,snapshots.ts}`, `apps/worker/test/profile/build-profile.test.ts`, `apps/worker/test/profile/seed.ts` (a synthetic seed reused by Task 26)
- Modify:
  - `apps/worker/src/queue/queues.ts`: queues `build-profile` (payload `{ accountId; timeClass }`, jobId `profile-<accountId>-<timeClass>`, delay 30 s, `removeOnComplete/Fail: true`), `build-explorer`, `build-band-reference`, `analyse-reference-game`; repeatable jobs `build-explorer` and `build-band-reference` on cron `0 3 1 1,4,7,10 *`.
  - `apps/worker/src/events/publish.ts`: add `"profile.updated" | "weakness.changed"` to the type union.
  - `apps/worker/src/start-workers.ts`:
    - TB pool from `SYZYGY_PATH` (optional);
    - `selectDifficultyProvider`;
    - `enrich` deps for analyse-game;
    - `onAnalysed` → enqueue build-profile;
    - all new workers (build-profile concurrency 2, reference jobs at the analyse pool's lowest priority).
  - `apps/worker/src/main.ts` (env `DATA_DIR`, `SYZYGY_PATH`, `MAIA_URL`).

**Interfaces:**
- Consumes: Tasks 13, 14 (`loadBandReference`), 16, 18, 19, 23, 24.
- Produces: `handleBuildProfile(deps: { db; redis; queues; now?: () => Date }, payload: { accountId; timeClass }): Promise<{ estimates: number; snapshots: number; transitions: number }>`. Steps:
  1. Record `startedAt`.
  2. Load games, series and the band reference (player band from the latest rating via `toLichessEquivalent`).
  3. Compute `estimateWeakness` for every binomial key and upsert `weaknessEstimates`.
  4. Run `evaluateSeries` per series. Append `weaknessStateHistory` rows whose `(kind, to, atGameId)` are not yet stored. Update `weaknessEstimates.state/pImp`.
  5. Compute `computeKpis` for `last50` and for every ISO week containing a game analysed since the last snapshot. Append a row only when `value` or `n` differ from the latest row for that metric and window.
  6. Compute `computeAreaScores` and append them as `area:<area>` snapshots.
  7. Run `rebuildRepertoire`.
  8. Publish, after commit: one `profile.updated { timeClass, area, kpis: [{metric, value, n, ciLow, ciHigh}] }` per area whose snapshot rows changed, and one `weakness.changed { id: seriesKey, timeClass, state, fromState, onsetAt }` per new transition row.
  9. If any game for the account × TC has `analysedAt > startedAt`, re-enqueue itself.
  10. Enqueue `schedule-analysis`.

- [ ] **Step 1: Write failing tests** (DB). The seed inserts an account, 40 analysed blitz games with `game_metrics` and series points (W1 at 0.04 vs a band reference of 0.02), and one active band-reference build:
  - after a run there is a `weakness_estimates` row per binomial key; W1 has `confidence > 0.7` and state `active`;
  - `kpi_snapshot` has rows for `last50` and the 2 seed weeks;
  - `user_events` holds 6 `profile.updated` events (mindset has no benchmark, so no change event) and a `weakness.changed` for W1;
  - a second identical run appends 0 snapshot rows and 0 history rows and publishes no `weakness.changed`;
  - a game with `analysedAt` after `startedAt` (fake clock) causes exactly one re-enqueue;
  - with no band reference build: estimates have `reason` no_benchmark, states are all `not_an_issue`, and the run does not throw;
  - `startWorkers` boots with `SYZYGY_PATH` and `MAIA_URL` unset (smoke).
- [ ] **Step 2:** Run the tests in the container. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the whole worker suite and `pnpm -w typecheck && pnpm -w lint`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(worker): build-profile job with snapshots, timeline and events`.

### Task 26 (I): `CoachingInput` builder, CLI and ADR index

**Files:**
- Create: `apps/worker/src/profile/{coaching-input.ts,exemplars.ts,strengths.ts}`, `apps/worker/test/profile/coaching-input.test.ts`, `docs/decisions/0043-build-profile-and-coaching-input.md`
- Modify: `apps/worker/src/cli.ts` (subcommand `profile`; create a minimal `parseArgs` CLI if M1 Task 13 has not landed), `docs/decisions/README.md` (index 0029–0043 as present), `CLAUDE.md` (one line each: `data/` directory, fetch commands)

**Interfaces:**
- Consumes: everything above.
- Produces:
  - `buildCoachingInput(db, accountId: string, timeClass: TimeClass, opts?: { now?: Date }): Promise<CoachingInput>`. Field rules:
    - `timeControl` = the most common `(base, inc)` in the window, null for daily.
    - `rating` = the latest rating; `band` from `toLichessEquivalent`.
    - `basis`:
      - `importedGames` = all games for the account × TC;
      - `analysedGames` = `done` games;
      - window = analysed min/max `playedAt`;
      - `backfillComplete` = no `pending`/`analysing` games and no further selection possible (`selectForAnalysis` returns []).
    - `areaScores` = the latest `area:*` values (all 7). `metrics` = the latest `last50` snapshots, mapped to M3 `Metric` (label/unit from the catalogue, `bandMedian` and `bandPercentile` from the reference, `sufficient = n ≥ minN`, `needMore`).
    - `weaknesses` = `rankWeaknesses`, with key mapped to `id`/`variant` and `rank` 1-based. Exemplars: up to 5 for ranks 1–3 and 2 for the rest, taken from `exemplarPlies` within 6 months, ordered by ΔW then recency, distinct games.
    - `openings` = `openingLines` with a non-null verdict. `movesSan` comes from the line entry path; each verdict gets up to 3 examples (the fix spot or the worst ΔW in the line).
    - `changes` = history rows from the last 90 days that have an onset, with `candidateCauses` and up to 3 examples on each side of the onset.
    - `strengths` (controller ruling), at most 4, sorted by percentile:
      - areas with score ≥ 60 and weaknesses with `1 − confidence ≥ 0.8`;
      - each needs ≥ 1 exemplar of the matching kind (tactics → `tactic_found`, calculation → `only_move_found`, middlegame → `conversion`/`defence`, endgame → `conversion` in the endgame phase);
      - entries without an exemplar are dropped.
  - `toExemplarPosition(row): ExemplarPosition` (white-POV `evalBefore`/`evalAfter`, SAN via chess.js)
  - CLI: `pnpm --filter @ci/worker cli profile --platform <p> --username <u> --time-class <tc> [--json]` prints `band B2  analysed 214/3912  focus-ready weaknesses N  areas: openings 54 …` or, with `--json`, the whole input.

- [ ] **Step 1: Write failing tests** (reusing Task 25's seed with real moves for exemplars):
  - `areaScores.map(a => a.area)` equals `AREAS`;
  - `weaknesses` priorities are non-increasing and `rank` starts at 1;
  - W1 has 3 exemplars;
  - every exemplar's `bestPv[0] === best.uci`, and replaying `played.uci` from `fen` with chess.js gives `played.san`;
  - every `refutationPv` replays legally from `fen` + `played`;
  - `basis = { analysedGames: 40, importedGames: 40, … }`;
  - no `focus` key;
  - every strength has 1–3 exemplars;
  - `coaching input for an account with 0 analysed games`: `weaknesses []`, `changes []`, `strengths []`, 7 `areaScores` with `sufficient: false`, and no throw;
  - the JSON survives `JSON.parse(JSON.stringify(x))` unchanged (no Dates, no Maps).
- [ ] **Step 2:** Run the tests in the container. Expected: FAIL.
- [ ] **Step 3:** Implement. Write ADR 0043, covering:
  - the build-profile flow;
  - event payloads;
  - append-only rules;
  - the `CoachingInput` contract and its mapping to M3 (including `strengths`).

  Update the ADR index and `CLAUDE.md`.
- [ ] **Step 4: Live smoke.** Run:
  1. `docker compose run --rm worker pnpm --filter @ci/worker cli analyse --platform lichess --username DrNykterstein --max 40`
  2. `... cli profile --platform lichess --username DrNykterstein --time-class blitz --json > /tmp/ci.json` (inside the container)
  3. `jq '.basis.analysedGames, (.areaScores|length), (.weaknesses|length)'`

  Expected: analysed > 0, area scores 7, and the command exits 0. Areas say "no benchmark" until `build-band-reference` has run; that is acceptable and stated in the output.
- [ ] **Step 5:** Run `pnpm -w lint && pnpm -w typecheck && docker compose run --rm worker pnpm test`. Expected: PASS.
- [ ] **Step 6:** Commit `feat(worker): coaching input builder, profile cli and adr index`.

---

## Self-review

1. **Spec coverage.**

| Requirement | Task |
|---|---|
| Motif tagger (§8.3, §4.4, our own rules) | 4 |
| Threat probe (§4.4) | 3, 5, 22 |
| MultiPV on critical positions | 22 |
| Episodes W5, W6, W9a | 6 |
| Detectors W1–W12 | 7 |
| Beta posteriors, priors, P_imp, priority (§5.2–5.4, §6.6) | 16 |
| Band references (§6.1#8, §8#6) | 14 |
| Own explorer (§14) | 11 |
| Repertoire, struggles, verdicts (§8a) | 12, 13 |
| Syzygy (§14) | 20 |
| Maia plus fallback, licence first (§8 cont., §10a.6) | 21 |
| Area scores (§8b) | 19 |
| `game_metrics` / `kpi_snapshot` with Wilson, bootstrap, opponent adjustment (§12a) | 8, 19, 25 |
| Timeline with BOCPD, CUSUM, states, onset, causes, tables (§12b) | 17, 18, 25 |
| Scheduler P0–P3, adaptive stopping, stratified sample, imported status (§8c) | 23 |
| build-profile and events (§13a) | 25 |
| CoachingInput (§9, §6.3) | 1, 26 |
| Strength exemplars (controller) | 9, 26 |

   Gaps left explicitly for later milestones: playstyle axes and style-clash `replace` (no task; ADR 0041 defers them); fatigue and plan-based P_imp (M6/M7); EWMA drift on plans (M7); the rating-conditioned expected-score fit (§4.3; T_win is a constant, ADR 0037); pooled-user benchmarks replacing Lichess references (later).
2. **Step scan.** Every test step lists assertions with values. Code steps give signatures and pinned constants. Bodies appear only for BOCPD, `nextState` and the settled-swing definition. The repertoire DB step names its exact checks.
3. **Type consistency.** These names are used the same way in every task: `RawMoveFeatures`, `MoveFeatures`, `LineAnalysis`, `EnrichedMove`, `AnalysedGame`, `GameObservation`, `BinomialObs`, `GameMetrics.strengths`, `BandReferenceStat`, `TablebaseProbe`, `DifficultyProvider`, `SeriesKey`, `WeaknessEstimate`, `selectCriticalPlies`, `enrichGame`, `loadExplorerIndex`, `loadBandReference`, `computeGameFeatures`, `handleBuildProfile` and `buildCoachingInput`. `CandidateCause` kinds use M3's names in Tasks 1, 18 and 26.
4. **Review Focus.** Each of the five items has a named test in its owning task (Tasks 22, 3, 16/19/26, 7/19 and 22/20).
5. **Proportion.** The plan is long because the roadmap row holds about 15 subsystems. Code blocks are limited to test assertions and three algorithms; everything else is signatures and rules.
