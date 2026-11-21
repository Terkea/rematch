# Foundation and Analysis Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Go from a Lichess or chess.com username to fully engine-analysed games stored in Postgres, with a per-move record for every ply, running entirely in Docker Compose.

**Architecture:** A pnpm + Turborepo TypeScript monorepo. `packages/chess-core` holds pure chess maths (win%, accuracy, judgement, PGN with clocks, phases, move records). `packages/db` holds the Drizzle schema. `apps/worker` holds importers, the Stockfish UCI pool, the eval cache, BullMQ jobs and a CLI. Postgres, Redis and the worker (with Stockfish 19 built into its image) run in Docker Compose.

**Tech Stack:** Node 22, TypeScript 5, pnpm 12, Turborepo 2.11, Vitest 5, Biome, chess.js 1.4, Drizzle ORM 0.45 + postgres.js, BullMQ 6, Redis 7, Postgres 17, Stockfish 19, Docker Compose.

**Spec:** `docs/superpowers/specs/2026-11-21-chess-improver-design.md` (§6, §7, §8, §13) and `docs/research/improvement-engine.md` §4.1 and §6.1. Roadmap: `docs/superpowers/plans/2026-11-21-roadmap.md`.

## Global Constraints

- Local stack = Docker Compose (owner's instruction). Every service and test that needs Postgres, Redis or Stockfish runs via `docker compose`.
- Package scope `@ci/*`: `@ci/chess-core`, `@ci/db`, `@ci/worker`.
- ESM only (`"type": "module"`), TypeScript `strict: true`, and `noUncheckedIndexedAccess: true`.
- Analysis protocol id: `ANALYSIS_PROTOCOL = "sf19-n300k-v1"`, at 300_000 nodes per position, MultiPV 1 (ADR 0007). Every stored eval carries the protocol id.
- Win% and judgement formulas are exactly those in improvement-engine §4.1. The constant is `0.00368208`. cp is clamped to ±1000, and mate counts as ±1000.
- Time class: est = base + 40·increment; bullet < 180 s, blitz < 480 s, rapid < 1500 s, classical ≥ 1500 s; chess.com daily = `daily`.
- Phases (improvement-engine §6.1 #7): opening until ply `max(20, bookExitPly)`, capped at 30. Endgame when each side's non-pawn material is ≤ 13 (Q=9, R=5, B=N=3), or total non-pawn material is ≤ 26 with no queens. Otherwise middlegame. Phases never go backwards.
- Outbound HTTP: one request at a time per platform. On a 429, wait 60 s and retry up to 3 times. `User-Agent: chess-improver/0.1 (+mailto:${CONTACT_EMAIL})`, where `CONTACT_EMAIL` comes from env and has no default.
- Only standard chess is imported. Variants (chess960, crazyhouse and others) are skipped.
- Every decision not covered by an existing ADR gets a new ADR in `docs/decisions/`, continuing from 0021.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Games without clocks** (chess.com daily, some Lichess correspondence games) must import and analyse with `clockBefore`/`timeSpent` = null, not crash. Test: Task 4 `parses game without clocks`, Task 5 `move records without clocks`.
2. **Aborted or very short games** (0–1 plies) must not reach the engine. They are marked `skipped`. Test: Task 12 `skips games with fewer than 2 plies`.
3. **A non-existent username or a network failure part-way through** must give a clear error and leave the DB consistent. Re-running an import must be idempotent. Tests: Task 7 `404 throws PlatformNotFoundError`, Task 12 `import twice yields same rows`.
4. **Mate scores and terminal positions** (checkmate, stalemate) must convert correctly to win%, must never be sent to the engine, and must judge "mate created" correctly. Tests: Task 3 `mate rules`, Task 11 `terminal positions not sent to engine`.
5. **Invalid FENs or a Stockfish crash** must not kill the worker. Stockfish 19 exits on bad input. Tests: Task 10 `rejects invalid FEN without killing engine`, `pool recovers after engine crash`.

---

### Task 1: Monorepo scaffold, tooling and CI

**Files:**
- Create: `package.json`, `pnpm-workspace.yaml`, `turbo.json`, `tsconfig.base.json`, `biome.json`, `vitest.workspace.ts`, `.nvmrc` (22), `.github/workflows/ci.yml`, `README.md`, `CLAUDE.md`
- Create: `packages/chess-core/{package.json,tsconfig.json,src/index.ts,test/smoke.test.ts}`
- Create: `docs/decisions/0021-typescript-not-rust.md`, `docs/decisions/0022-tooling-biome-vitest.md`; update `docs/decisions/README.md`

**Interfaces:**
- Produces: root scripts `pnpm build`, `pnpm test`, `pnpm lint`, `pnpm typecheck` (each via `turbo run <task>`); a package template that later packages copy.

- [ ] **Step 1:** Enable pnpm via `corepack enable && corepack prepare pnpm@12 --activate`, then run `pnpm --version`. Expected: `12.x`.
- [ ] **Step 2:** Write the root config files. The workspace globs are `apps/*`, `packages/*`, `services/*`. Turbo tasks: `build` (dependsOn `^build`, outputs `dist/**`), `test`, `lint`, `typecheck`.
- [ ] **Step 3:** Write the failing smoke test `packages/chess-core/test/smoke.test.ts`: `expect(VERSION).toBe("0.1.0")`, importing `VERSION` from `../src/index.js`.
- [ ] **Step 4:** Run `pnpm install && pnpm test`. Expected: FAIL, because `VERSION` is not exported.
- [ ] **Step 5:** Export `VERSION = "0.1.0"` from `packages/chess-core/src/index.ts`, then run `pnpm test && pnpm lint && pnpm typecheck`. Expected: all pass.
- [ ] **Step 6:** CI workflow on push/PR: checkout, setup-node 22, corepack pnpm, `pnpm install --frozen-lockfile`, lint, typecheck, test (DB tests get a postgres + redis service container in Task 6).
- [ ] **Step 7:** Write `CLAUDE.md`, under 40 lines: how to run the stack (`docker compose up -d`), how to run tests, the ADR rule, pointers to the spec and roadmap. Write ADR 0021 (TypeScript everywhere; Rust only for a profiled hot batch job, and note that Rust chess libs shakmaty/pgn-reader are GPL) and ADR 0022 (Biome for lint and format, Vitest for tests).
- [ ] **Step 8:** Commit `chore: scaffold monorepo with turbo, biome, vitest and CI`.

### Task 2: Docker Compose local stack

**Files:**
- Create: `docker-compose.yml`, `.env.example`, `apps/worker/Dockerfile`, `apps/worker/package.json` (minimal, `@ci/worker`), `apps/worker/src/main.ts` (placeholder that logs `worker ready` and the Stockfish version), `docker/postgres/init.sql`

**Interfaces:**
- Produces:
  - Compose services `postgres` (17, port 5432, databases `chess` and `chess_test`), `redis` (7, port 6379), and `worker` (build `apps/worker/Dockerfile`, target `dev`, the repo mounted at `/app`, `STOCKFISH_PATH=/usr/local/bin/stockfish`).
  - Env vars `DATABASE_URL`, `DATABASE_URL_TEST`, `REDIS_URL`, `STOCKFISH_PATH`, `CONTACT_EMAIL`, `LICHESS_TOKEN` (optional).
  - The command `docker compose run --rm worker pnpm test`, which runs the whole test suite with Stockfish, Postgres and Redis available.

- [ ] **Step 1:** Find the Linux Stockfish 19 asset by running `gh release view sf_19 -R official-stockfish/Stockfish --json assets -q '.assets[].name'`. Pick the x86-64 Linux asset and record its name and SHA-256.
- [ ] **Step 2:** Write the Dockerfile. Base `node:22-bookworm-slim`. Download the pinned asset, verify the SHA-256, and install it to `/usr/local/bin/stockfish`. Enable corepack pnpm. The `dev` target runs `pnpm install` then `pnpm --filter @ci/worker dev`.
- [ ] **Step 3:** Write `docker-compose.yml`. Postgres and Redis get healthchecks, and `worker` depends on both being healthy.
- [ ] **Step 4:** Run `docker compose up -d --build && docker compose logs worker`. Expected: `worker ready` and a `Stockfish 19` line.
- [ ] **Step 5:** Run `docker compose run --rm worker pnpm test`. Expected: the smoke test passes inside the container.
- [ ] **Step 6:** Commit `chore: docker compose stack with postgres, redis and stockfish 19 worker`.

### Task 3: chess-core win%, accuracy and judgement

**Files:**
- Create: `packages/chess-core/src/score.ts`, `packages/chess-core/src/judgement.ts`, `packages/chess-core/test/score.test.ts`, `packages/chess-core/test/judgement.test.ts`; export from `src/index.ts`

**Interfaces:**
- Produces:
  - `type EngineScore = { cp: number } | { mate: number }` (mate > 0 means the side whose POV it is mates)
  - `scoreToCp(s: EngineScore): number` (clamped to ±1000; mate becomes ±1000)
  - `flipScore(s: EngineScore): EngineScore`
  - `winPercent(s: EngineScore): number`
  - `winningChances(s: EngineScore): number`
  - `moveAccuracy(winBefore: number, winAfter: number): number`
  - `type Judgement = "inaccuracy" | "mistake" | "blunder"`
  - `judge(before: EngineScore, after: EngineScore): Judgement | null`, where both scores are from the **mover's** POV

- [ ] **Step 1: Write failing tests**

```ts
expect(winPercent({ cp: 0 })).toBe(50);
expect(winPercent({ cp: 100 })).toBeCloseTo(59.10, 1);
expect(winPercent({ cp: -100 })).toBeCloseTo(40.90, 1);
expect(winPercent({ cp: 2000 })).toBeCloseTo(winPercent({ cp: 1000 }), 10);
expect(winPercent({ mate: 3 })).toBeCloseTo(97.54, 1);
expect(moveAccuracy(50, 50)).toBe(100);
expect(moveAccuracy(50, 60)).toBe(100);
expect(moveAccuracy(60, 50)).toBeCloseTo(64.58, 1);
expect(moveAccuracy(100, 0)).toBe(0);
// judgement thresholds on winningChances drop
expect(judge({ cp: 0 }, { cp: -200 })).toBe("blunder");
expect(judge({ cp: 0 }, { cp: -120 })).toBe("mistake");
expect(judge({ cp: 0 }, { cp: -60 })).toBe("inaccuracy");
expect(judge({ cp: 0 }, { cp: -40 })).toBeNull();
// "mate rules"
expect(judge({ cp: -50 }, { mate: -2 })).toBe("blunder");
expect(judge({ cp: -800 }, { mate: -2 })).toBe("mistake");
expect(judge({ cp: -1000 }, { mate: -2 })).toBe("inaccuracy");
expect(judge({ mate: 2 }, { cp: 50 })).toBe("blunder");
expect(judge({ mate: 2 }, { cp: 800 })).toBe("mistake");
expect(judge({ mate: 2 }, { cp: 1000 })).toBe("inaccuracy");
expect(judge({ mate: 3 }, { mate: 2 })).toBeNull();
```

- [ ] **Step 2:** Run `pnpm --filter @ci/chess-core test`. Expected: FAIL (modules missing).
- [ ] **Step 3:** Implement following the formulas in improvement-engine §4.1. `moveAccuracy` includes the `+1` uncertainty bonus and is clamped to 0..100. In `judge`, "mate created" (before is not a mate and after is a negative mate) uses `scoreToCp(before)`: < −999 gives inaccuracy, < −700 gives mistake, otherwise blunder. "Mate lost" (before is a positive mate and after is not a positive mate) uses `scoreToCp(after)`: > 999 gives inaccuracy, > 700 gives mistake, otherwise blunder. In every other case, use the winningChances drop with thresholds 0.3/0.2/0.1.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): lichess win%, accuracy and judgement`.

### Task 4: chess-core time control, FEN key and PGN parsing with clocks

**Files:**
- Create: `packages/chess-core/src/time-control.ts`, `src/fen.ts`, `src/pgn.ts`, tests `test/time-control.test.ts`, `test/pgn.test.ts`, fixtures `test/fixtures/scholars-mate.pgn`, `test/fixtures/no-clocks.pgn`, `test/fixtures/chess960.pgn`

**Interfaces:**
- Produces:
  - `type TimeClass = "bullet" | "blitz" | "rapid" | "classical" | "daily"`
  - `classifyTimeControl(baseSeconds: number, incrementSeconds: number): Exclude<TimeClass, "daily">`
  - `fenKey(fen: string): string`, which returns the first four FEN fields
  - `type Color = "white" | "black"`
  - `interface Ply { ply: number; color: Color; san: string; uci: string; fenBefore: string; fenAfter: string; clockSeconds: number | null }`, where `ply` is 1-based and `clockSeconds` is the mover's clock after the move
  - `interface ParsedGame { headers: Record<string, string>; startFen: string; plies: Ply[]; terminal: "checkmate" | "stalemate" | "draw" | null }`
  - `parsePgn(pgn: string): ParsedGame`, which throws `UnsupportedVariantError` when the `Variant` header is not standard or a `SetUp`/`FEN` header is present

- [ ] **Step 1: Write failing tests.**
  - Time control: `classifyTimeControl(60,0)` is `"bullet"`; `(180,0)`, `(180,2)` and `(300,3)` are `"blitz"`; `(480,0)`, `(600,0)` and `(900,10)` are `"rapid"`; `(1800,0)` is `"classical"`.
  - FEN key: `fenKey("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1")` equals `"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq -"`.
  - PGN: Scholar's mate fixture (`1.e4 {[%clk 0:02:58]} e5 {[%clk 0:02:57]} 2.Bc4 {[%clk 0:02:55]} Nc6 {[%clk 0:02:54]} 3.Qh5 {[%clk 0:02:50]} Nf6 {[%clk 0:02:40]} 4.Qxf7# {[%clk 0:02:49]} 1-0`, with `TimeControl "180"`). Expect `plies.length === 7`, `plies[0]` = `{ ply:1, color:"white", san:"e4", uci:"e2e4", clockSeconds:178 }`, `plies[6].uci === "h5f7"`, `terminal === "checkmate"`.
  - `parses game without clocks`: every `clockSeconds` is null.
  - The chess960 fixture throws `UnsupportedVariantError`.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement with chess.js 1.4. Load the PGN, replay the moves on a fresh `Chess()`, read clocks from each move's comment with `/\[%clk (\d+):(\d+):(\d+(?:\.\d+)?)\]/`, and derive `uci` from `from+to+promotion`.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): time classes, fen keys and pgn parsing with clocks`.

### Task 5: chess-core phases, game accuracy and move records

**Files:**
- Create: `packages/chess-core/src/phase.ts`, `src/game-accuracy.ts`, `src/move-record.ts`, tests for each

**Interfaces:**
- Consumes: `EngineScore`, `winPercent`, `moveAccuracy`, `judge`, `flipScore` (Task 3); `ParsedGame`, `Ply`, `Color` (Task 4).
- Produces:
  - `type Phase = "opening" | "middlegame" | "endgame"`
  - `detectPhases(fensBefore: string[], bookExitPly?: number): Phase[]` (default bookExitPly 20)
  - `gameAccuracy(winPercentsWhitePov: number[], color: Color): number`
  - `interface PositionEval { score: EngineScore; bestUci: string | null; pv: string[] }`, where `score` is from **white's** POV and evals[i] is the position before ply i+1. The array is `plies.length + 1` long, and the last entry is the final position.
  - `interface MoveRecord { ply: number; color: Color; san: string; uci: string; fenBefore: string; bestUci: string | null; winBefore: number; winAfter: number; deltaW: number; accuracy: number; judgement: Judgement | null; phase: Phase; clockBefore: number | null; timeSpent: number | null; decided: boolean }`. `winBefore`/`winAfter` are from the mover's POV, and `deltaW = max(0, winBefore − winAfter)`. `decided` is `|winBefore − 50| ≥ 40`.
  - `buildMoveRecords(game: ParsedGame, evals: PositionEval[], clock: { baseSeconds: number; incrementSeconds: number } | null): MoveRecord[]`

- [ ] **Step 1: Write failing tests.**
  - Phases:
    - the start position at plies 1..25 gives opening for plies 1–20 and middlegame from 21;
    - `"8/5k2/8/8/3R4/8/2r5/4K3 w - - 0 1"` gives endgame;
    - a position with Q+R each side and no other pieces (14 each, 28 total) gives middlegame;
    - a sequence endgame → richer position stays endgame;
    - `bookExitPly: 26` keeps opening through ply 26;
    - `bookExitPly: 40` is capped at 30.
  - Game accuracy: all-equal win% (50) for both colours gives 100. A constant series with one big drop for black gives `gameAccuracy(...,"black") < gameAccuracy(...,"white")`.
  - Move records on Scholar's mate with evals (white POV, 8 entries) `[20,30,25,30,40,50]` cp, then `{mate:1}`, then `{cp:1000}`:
    - `records[5]` (Nf6, black) has `judgement: "blunder"` and `deltaW > 40`;
    - `records[0].timeSpent === 2` (180 − 178 + 0);
    - `records[2].timeSpent === 3` (178 − 175);
    - `records[0].phase === "opening"`.
  - `move records without clocks`: with `clock: null`, every `clockBefore` and `timeSpent` is null.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement. Phases use non-pawn material counted from the FEN board field. `gameAccuracy` follows the volatility-weighted plus harmonic mean in improvement-engine §4.1. `buildMoveRecords` converts the white-POV evals to the mover's POV with `flipScore` for black. `timeSpent = previousClockOfSameColor − clockSeconds + increment`, and the first move of each colour uses `baseSeconds` as the previous clock.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(chess-core): phases, game accuracy and per-move records`.

### Task 6: Database schema and test harness

**Files:**
- Create: `packages/db/{package.json,tsconfig.json,drizzle.config.ts,src/schema.ts,src/client.ts,src/index.ts,test/schema.test.ts,test/setup.ts}`, plus generated `packages/db/drizzle/*.sql`
- Modify: `.github/workflows/ci.yml` (add postgres 17 and redis 7 service containers, set `DATABASE_URL_TEST`)

**Interfaces:**
- Produces: `createDb(url: string)` returning a Drizzle instance, plus these tables, exported from `@ci/db`:
  - `linkedAccounts`: `id uuid pk`, `userId uuid null`, `platform enum('lichess','chesscom')`, `username text` (lower-cased), `displayName text`, `verifiedAt timestamptz null`, `lastSyncedAt timestamptz null`, `createdAt`. Unique on `(platform, username)`.
  - `games`: `id uuid pk`, `accountId fk`, `platform`, `platformGameId text`, `pgn text`, `playedAt timestamptz`, `timeClass enum`, `baseSeconds int null`, `incrementSeconds int null`, `rated bool`, `playerColor enum('white','black')`, `whiteName`, `blackName`, `whiteRating int null`, `blackRating int null`, `result enum('1-0','0-1','1/2-1/2','*')`, `termination text null`, `eco text null`, `openingName text null`, `lichessEvals jsonb null`, `analysisStatus enum('pending','analysing','done','failed','skipped')` default pending, `analysisProtocol text null`, `analysisError text null`, `whiteAccuracy real null`, `blackAccuracy real null`. Unique on `(platform, platformGameId)`. Index on `(accountId, playedAt desc)`.
  - `positionEvals`: `fenKey text`, `protocol text`, `scoreCp int null`, `scoreMate int null` (white POV), `bestUci text null`, `pv text[]`, `source enum('stockfish','lichess','dump')`, `createdAt`. Primary key `(fenKey, protocol)`.
  - `moves`: `gameId fk cascade`, `ply int`, and every `MoveRecord` field as a column (snake_case). Primary key `(gameId, ply)`.
  - `upsertGame(db, row): Promise<{ id: string; inserted: boolean }>`, idempotent on `(platform, platformGameId)`.
- Test helper `test/setup.ts`: migrates `DATABASE_URL_TEST` and truncates all tables before each test file.

- [ ] **Step 1:** Write failing tests: `upsertGame` twice with the same platform game id returns the same `id`, with `inserted` true then false, and `select count(*) from games` is 1. Insert `moves` for a game, delete the game, and the moves cascade away. Inserting `positionEvals` with the same `(fenKey, protocol)` twice raises a unique violation.
- [ ] **Step 2:** Run `docker compose run --rm worker pnpm --filter @ci/db test`. Expected: FAIL.
- [ ] **Step 3:** Write the schema, then generate migrations with `pnpm --filter @ci/db drizzle-kit generate`. Use the `postgres` (postgres.js) driver.
- [ ] **Step 4:** Run the tests in the container. Expected: PASS.
- [ ] **Step 5:** Commit `feat(db): core schema for accounts, games, evals and moves`.

### Task 7: Platform HTTP client

**Files:**
- Create: `apps/worker/src/http/platform-client.ts`, `apps/worker/test/http/platform-client.test.ts`

**Interfaces:**
- Produces:
  - `interface PlatformClient { getJson<T>(url: string, init?: { headers?: Record<string,string> }): Promise<T>; getNdjson<T>(url: string, init?: { headers?: Record<string,string> }): AsyncIterable<T> }`
  - `createPlatformClient(opts: { name: "lichess" | "chesscom"; contactEmail: string; fetch?: typeof fetch; retryAfterMs?: number; maxRetries?: number }): PlatformClient` (defaults: retryAfterMs 60_000, maxRetries 3)
  - `class PlatformNotFoundError extends Error`
  - `class PlatformRateLimitError extends Error`

- [ ] **Step 1:** Write failing tests using an injected fake `fetch`:
  - `sends identifying user agent`: the header equals `chess-improver/0.1 (+mailto:ops@example.com)`.
  - `serialises requests`: two concurrent calls never overlap (a fake fetch that tracks in-flight count, maximum 1).
  - `retries after 429`: the fake returns 429 then 200, with `retryAfterMs: 0`, and the call resolves.
  - Four 429s throw `PlatformRateLimitError`.
  - `404 throws PlatformNotFoundError`.
  - `getNdjson` yields three objects from a body split mid-line across chunks.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement with a promise-chain mutex per client. NDJSON goes through a `TextDecoderStream` with line buffering.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(worker): serial rate-limited platform http client`.

### Task 8: Lichess importer

**Files:**
- Create: `apps/worker/src/importers/types.ts`, `apps/worker/src/importers/lichess.ts`, `apps/worker/test/importers/lichess.test.ts`, fixture `apps/worker/test/fixtures/lichess-games.ndjson`

**Interfaces:**
- Consumes: `PlatformClient` (Task 7), `classifyTimeControl` and `TimeClass` (Task 4).
- Produces:
  - `interface NormalizedGame { platform: "lichess" | "chesscom"; platformGameId: string; pgn: string; playedAt: Date; timeClass: TimeClass; baseSeconds: number | null; incrementSeconds: number | null; rated: boolean; white: { name: string; rating: number | null }; black: { name: string; rating: number | null }; result: "1-0" | "0-1" | "1/2-1/2" | "*"; termination: string | null; eco: string | null; openingName: string | null; lichessEvals: (EngineScore | null)[] | null }`, where `lichessEvals[i]` is the white-POV eval after ply i+1
  - `fetchLichessGames(client: PlatformClient, username: string, opts?: { since?: Date; max?: number; token?: string }): AsyncIterable<NormalizedGame>`

- [ ] **Step 1:** Record the fixture by running `curl -s -H "Accept: application/x-ndjson" "https://lichess.org/api/games/user/DrNykterstein?max=3&pgnInJson=true&clocks=true&evals=true&opening=true"` and saving the output. Then append one hand-edited line with `"variant":"chess960"`.
- [ ] **Step 2:** Write failing tests against a fake client serving the fixture:
  - three games are yielded and the chess960 line is skipped;
  - the requested URL contains `pgnInJson=true`, `clocks=true`, `evals=true`, `opening=true`, `sort=dateDesc` and `max=3`;
  - `since` becomes epoch milliseconds;
  - `token` becomes an `Authorization: Bearer` header;
  - `timeClass`, `baseSeconds` and `incrementSeconds` are mapped from `clock.initial`/`clock.increment` (correspondence games give `"daily"` with nulls);
  - `result` is derived from `winner`/`status`;
  - a game with an `analysis` array has `lichessEvals` of length equal to the ply count, with `mate` entries mapped to `{mate}`.
- [ ] **Step 3:** Run the tests. Expected: FAIL.
- [ ] **Step 4:** Implement. Lower-case the username in the URL.
- [ ] **Step 5:** Run the tests. Expected: PASS.
- [ ] **Step 6:** Commit `feat(worker): lichess game importer`.

### Task 9: chess.com importer

**Files:**
- Create: `apps/worker/src/importers/chesscom.ts`, `apps/worker/test/importers/chesscom.test.ts`, fixtures `apps/worker/test/fixtures/chesscom-archives.json`, `chesscom-month.json`

**Interfaces:**
- Consumes: `PlatformClient`, `NormalizedGame`.
- Produces: `fetchChesscomGames(client: PlatformClient, username: string, opts?: { since?: Date; max?: number }): AsyncIterable<NormalizedGame>`

- [ ] **Step 1:** Record the fixtures with `curl -s -A "chess-improver/0.1 (+mailto:$CONTACT_EMAIL)"` from `https://api.chess.com/pub/player/hikaru/games/archives` and the latest month URL. Trim the month to 4 games and add one with `"rules":"chess960"` and one daily game (`"time_control":"1/86400"`).
- [ ] **Step 2:** Write failing tests:
  - archives are walked newest first;
  - games within a month are yielded newest first (`end_time` descending);
  - iteration stops at `max`;
  - months entirely before `since` are not fetched;
  - the chess960 game is skipped;
  - `"180+2"` maps to base 180 and increment 2, blitz;
  - `"1/86400"` maps to `"daily"` with base 86400 and increment 0;
  - `platformGameId` is the last path segment of `url`;
  - `result` comes from the PGN `Result` header;
  - `lichessEvals` is null.
- [ ] **Step 3:** Run the tests. Expected: FAIL.
- [ ] **Step 4:** Implement.
- [ ] **Step 5:** Run the tests. Expected: PASS.
- [ ] **Step 6:** Commit `feat(worker): chess.com game importer`.

### Task 10: Stockfish UCI engine and pool

**Files:**
- Create: `apps/worker/src/engine/uci-engine.ts`, `apps/worker/src/engine/engine-pool.ts`, `apps/worker/test/engine/uci-engine.test.ts`, `engine-pool.test.ts`

**Interfaces:**
- Produces:
  - `interface AnalysisLine { multipv: number; score: EngineScore; pv: string[]; depth: number; nodes: number }`, where `score` is from the side-to-move's POV
  - `class InvalidFenError extends Error`
  - `class UciEngine { static start(opts: { path: string; threads?: number; hashMb?: number; syzygyPath?: string }): Promise<UciEngine>; analyse(fen: string, opts: { nodes: number; multipv?: number }): Promise<AnalysisLine[]>; readonly alive: boolean; quit(): Promise<void> }`
  - `class EnginePool { static create(opts: { size: number; path: string; hashMb?: number }): Promise<EnginePool>; analyse(fen: string, opts: { nodes: number; multipv?: number }): Promise<AnalysisLine[]>; close(): Promise<void> }`
  - `ENGINE_NODES = 300_000` and `ANALYSIS_PROTOCOL = "sf19-n300k-v1"` exported from `apps/worker/src/engine/protocol.ts`

- [ ] **Step 1:** Write failing tests (run in the container with the real binary):
  - the start position at 50_000 nodes gives `lines[0].score` cp within ±100 and `pv[0]` matching `/^[a-h][1-8][a-h][1-8][qrbn]?$/`;
  - `"6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1"` gives `{ mate: 1 }` and best `d1d8`;
  - `multipv: 3` returns 3 lines;
  - `rejects invalid FEN without killing engine`: `"not a fen"` rejects with `InvalidFenError`, then the next valid analyse succeeds;
  - `pool recovers after engine crash`: kill one engine's child process (`SIGKILL`), and the pool's next 4 analyses all succeed;
  - with pool size 2, two parallel analyses both resolve.
- [ ] **Step 2:** Run `docker compose run --rm worker pnpm --filter @ci/worker test engine`. Expected: FAIL.
- [ ] **Step 3:** Implement. Spawn the child process and run the `uci`/`uciok`, `setoption` and `isready` handshake. For each analysis, send `ucinewgame`, then `position fen`, then `go nodes N`, collect the last `info` line for each multipv, and resolve on `bestmove`. Validate FENs with chess.js `validateFen` before sending. The pool keeps a queue of idle engines; when an engine dies it restarts it and retries the job once.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(worker): supervised stockfish uci engine pool`.

### Task 11: Eval cache and game evaluation

**Files:**
- Create: `apps/worker/src/analysis/eval-cache.ts`, `apps/worker/src/analysis/evaluate-game.ts`, tests for each

**Interfaces:**
- Consumes: `EnginePool` and `ANALYSIS_PROTOCOL` (Task 10), `positionEvals` (Task 6), `ParsedGame`, `PositionEval`, `fenKey` and `flipScore` (Tasks 4–5).
- Produces:
  - `interface EvalCache { getMany(fenKeys: string[], protocol: string): Promise<Map<string, PositionEval>>; putMany(entries: { fenKey: string; protocol: string; eval: PositionEval; source: "stockfish" | "lichess" }[]): Promise<void> }`
  - `createDbEvalCache(db): EvalCache`
  - `createMemoryEvalCache(): EvalCache`, for tests
  - `evaluateGame(deps: { pool: Pick<EnginePool,"analyse">; cache: EvalCache }, game: ParsedGame, lichessEvals: (EngineScore | null)[] | null): Promise<PositionEval[]>`, returning `plies.length + 1` white-POV evals

- [ ] **Step 1:** Write failing tests with a fake pool that records FENs:
  - all positions are analysed once and the results are stored in the cache;
  - a second call on the same game makes 0 engine calls;
  - `terminal positions not sent to engine`: Scholar's mate's final position gets `{cp:1000}` without an engine call, and a stalemate final position gets `{cp:0}`;
  - when the engine returns `{cp:30}` with black to move, the stored white-POV score is `{cp:-30}`;
  - when `lichessEvals` is complete, the engine is called 0 times, and the start position comes from the cache or the engine (the Lichess array starts after ply 1);
  - duplicate FENs within a game are analysed once;
  - DB cache round-trip test against `DATABASE_URL_TEST`.
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3:** Implement. Batch cache lookups per game, send only the misses to the pool at `ENGINE_NODES`, and put results with `source:"stockfish"`, or `"lichess"` for Lichess evals (stored under the same protocol id, an accepted approximation recorded in ADR 0023).
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Write ADR 0023 "Lichess server evals accepted into the eval cache". Commit `feat(worker): eval cache and per-game evaluation`.

### Task 12: Queues, job handlers and persistence

**Files:**
- Create: `apps/worker/src/queue/queues.ts`, `apps/worker/src/jobs/import-account.ts`, `apps/worker/src/jobs/analyse-game.ts`, `apps/worker/src/main.ts` (replace the placeholder), tests `apps/worker/test/jobs/*.test.ts`

**Interfaces:**
- Consumes: the importers (Tasks 8–9), `upsertGame` and the tables (Task 6), `parsePgn`, `buildMoveRecords` and `gameAccuracy` (Tasks 4–5), `evaluateGame` (Task 11).
- Produces:
  - Queue names `"import-account"` (payload `{ accountId: string; max?: number }`) and `"analyse-game"` (payload `{ gameId: string }`). Defaults: `attempts: 3`, exponential `backoff` of 5000 ms.
  - `handleImportAccount(deps: { db; importers: { lichess: typeof fetchLichessGames; chesscom: typeof fetchChesscomGames }; clients; enqueueAnalyse(gameId: string): Promise<void> }, payload): Promise<{ imported: number; skipped: number }>`
  - `handleAnalyseGame(deps: { db; pool; cache }, payload): Promise<{ status: "done" | "skipped" }>`
  - `startWorkers(env): Promise<{ close(): Promise<void> }>`

- [ ] **Step 1:** Write failing integration tests (real Postgres and Redis, fake importer and fake pool):
  - `import twice yields same rows`: a fake importer yields 2 games, and running the import twice leaves 2 `games` rows, 2 enqueued analyses on the first run and 0 new ones on the second;
  - `playerColor` is set by matching the account username (case-insensitive) to the white or black name;
  - `lastSyncedAt` is updated;
  - the analysis of Scholar's mate writes 7 `moves` rows, sets `analysisStatus: "done"`, `analysisProtocol: "sf19-n300k-v1"`, and both accuracies non-null;
  - `skips games with fewer than 2 plies`: status `"skipped"`, 0 engine calls;
  - an unsupported variant PGN gives `"skipped"`;
  - an engine error sets `"failed"` with `analysisError` set and rethrows (so BullMQ retries);
  - a re-run after failure succeeds and leaves no duplicate `moves` rows (moves are replaced in a transaction).
- [ ] **Step 2:** Run the tests in the container. Expected: FAIL.
- [ ] **Step 3:** Implement. `main.ts` reads env, creates the pool (`size = os.cpus().length - 1`, minimum 1) and starts the BullMQ workers (`analyse-game` concurrency = pool size, `import-account` concurrency 2).
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(worker): import and analysis jobs on bullmq`.

### Task 12b: Crash recovery, reconciler, graceful shutdown and user events

Added after the plan was written. The owner requires an async pipeline that detects crashes and picks work back up, and a UI that receives results as they surface (spec §13a, ADR 0026).

**Files:**
- Modify: `packages/db/src/schema.ts` (+ a generated migration).
  - `games` gets `analysisStartedAt timestamptz null`, `heartbeatAt timestamptz null`, `analysisAttempts int default 0`.
  - `linkedAccounts` gets `importCursor jsonb null` (`{ oldestReached: string | null; newestSeenAt: string | null }`).
  - A new table `importRuns` (`id uuid pk`, `accountId fk`, `status enum('running','done','failed')`, `imported int`, `startedAt`, `heartbeatAt`, `finishedAt null`, `lastError text null`).
  - A new table `userEvents` (`id bigserial pk`, `accountId uuid`, `type text`, `payload jsonb`, `createdAt`), indexed on `(accountId, id)`.
- Create: `apps/worker/src/jobs/reconcile.ts`, `apps/worker/src/events/publish.ts`, `apps/worker/src/lifecycle/shutdown.ts`, `apps/worker/src/http/circuit-breaker.ts`, tests `apps/worker/test/recovery/*.test.ts`
- Modify: `apps/worker/src/jobs/import-account.ts`, `apps/worker/src/jobs/analyse-game.ts`, `apps/worker/src/main.ts`, `apps/worker/src/queue/queues.ts`

**Interfaces:**
- Consumes: Task 12 handlers, queues and `startWorkers`; Task 7 `PlatformClient` and its typed errors.
- Produces:
  - `publishEvent(deps: { db; redis }, accountId: string, type: "import.progress" | "game.analysed" | "job.failed", payload: object): Promise<number>`. It inserts into `userEvents` *after* the caller's commit, then `PUBLISH user-events:<accountId>`, and returns the event id.
  - `reconcile(deps: { db; queues; now?: () => Date }, opts?: { staleAfterMs?: number }): Promise<{ requeuedPending: number; resetStale: number; resumedImports: number }>`. `staleAfterMs` defaults to 600_000.
  - `createCircuitBreaker(opts: { failureThreshold: number; cooldownMs: number }): { exec<T>(fn: () => Promise<T>): Promise<T>; state(): "closed" | "open" | "half-open" }`. Defaults: threshold 5, cooldown 60_000. `CircuitOpenError` when open.
  - `installGracefulShutdown(handles: { workers; pool; db; redis }, opts?: { graceMs?: number }): void`. `graceMs` defaults to 25_000.
  - Queue `"reconcile"`: a repeatable job every 5 minutes. BullMQ worker options: `lockDuration: 60_000`, `maxStalledCount: 2`.

- [ ] **Step 1: Write failing tests** (real Postgres and Redis in the container):
  - `kill mid-analysis still completes`:
    - Start the analyse worker with a fake pool that hangs on position 4.
    - Force-close the worker (simulating a crash).
    - Start a fresh worker. After the stalled-job pickup, the game is `done`, `moves` has exactly one row per ply, and the fake pool saw the first positions only once (they came from the cache).
  - `reconciler requeues lost pending`: insert a `pending` game with no job in Redis; `reconcile` returns `requeuedPending: 1` and the job exists.
  - `reconciler resets stale analysing`: a game `analysing` with `heartbeatAt` 11 minutes ago becomes `pending` and is re-enqueued. One with a 1-minute-old heartbeat is untouched.
  - `import resumes from cursor`:
    - The fake importer throws after 2 of 4 games.
    - The `importCursor` reflects the 2 that were imported.
    - A re-run asks the importer for games only beyond the cursor, and ends with 4 rows and no duplicates.
  - `poison game fails after 3 attempts`: an unparseable PGN that throws a non-variant error gives `failed`, `analysisAttempts: 3`, `analysisError` set, and a `job.failed` event published.
  - `circuit opens after 5 failures`: 5 failing calls lead to `state() === "open"` and the 6th rejects with `CircuitOpenError` without calling fn. After the cooldown it goes half-open, and one success closes it.
  - `SIGTERM loses nothing`: during a running analysis, the shutdown handler resolves within `graceMs`. The game is either `done` or back to `pending` (never stuck in `analysing` without a live job), and every engine pid is dead.
  - `events are ordered and replayable`: 3 events for an account have increasing ids. A subscriber on `user-events:<accountId>` receives all 3. `select ... where id > <first>` returns the last 2.
- [ ] **Step 2:** Run `docker compose run --rm worker pnpm --filter @ci/worker test recovery`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - The analyse job sets `analysing`, `analysisStartedAt` and an incremented `analysisAttempts`, and updates `heartbeatAt` at least every 15 s while it evaluates (from inside the evaluate loop).
  - The import job writes `importRuns` and `importCursor` after each batch of 50 games, and calls the platform through a per-platform circuit breaker.
  - Events are published after commit only.
  - `main.ts` registers the repeatable reconcile job and installs graceful shutdown.
  - Write ADR 0026: "State in Postgres, queue as delivery; reconciler instead of an outbox".
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(worker): crash recovery, reconciler, graceful shutdown and user events`.

### Task 13: CLI and end-to-end live smoke test

**Files:**
- Create: `apps/worker/src/cli.ts`, `apps/worker/test/cli.test.ts`
- Modify: `apps/worker/package.json` (script `"cli": "tsx src/cli.ts"`), `README.md` (usage)

**Interfaces:**
- Consumes: everything above.
- Produces: `pnpm --filter @ci/worker cli analyse --platform <lichess|chesscom> --username <name> [--max N]`. It creates or finds the linked account, enqueues the import, runs the workers in-process until the queues drain, and prints a summary: `games: N  analysed: N  skipped: N  failed: N  avg accuracy: XX.X  blunders/game: X.XX`. It exits with code 0 on success and 1 when every game failed or the user is not found (with message `User "<name>" not found on <platform>`).

- [ ] **Step 1:** Write a failing test that runs the CLI's exported `runAnalyse(args, deps)` with fake importers and a fake pool, and asserts the summary line format and the counts. A not-found error returns exit code 1 with the message.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3:** Implement with `node:util` `parseArgs`.
- [ ] **Step 4:** Run it. Expected: PASS.
- [ ] **Step 5: Live smoke.** Run `docker compose run --rm worker pnpm --filter @ci/worker cli analyse --platform lichess --username DrNykterstein --max 5`, then the same with `--platform chesscom --username hikaru --max 5`. Expected: both print summaries with `analysed: 5` (or skipped variants counted) and plausible accuracies (60–99). Then run `docker compose exec postgres psql -U postgres chess -c "select count(*) from moves"`. Expected: > 0.
- [ ] **Step 6:** Commit `feat(worker): analyse cli with live smoke verification`.
