import type { Db } from "@ci/db";
import {
  gameMetrics,
  kpiSnapshot,
  moveFeatures,
  reportRuns,
  weaknessEstimates,
  weaknessSeriesPoint,
  weaknessStateHistory,
} from "@ci/db/generated";
import type { TimeClass } from "../../src/server/games/types";
import { assertSeedable, seedSyntheticGame } from "./games";

// Test-only writers for the profile read model (M5b Task 1). Every row is synthetic; each writer
// runs `assertSeedable` (through M5a's helpers or directly) before it writes.

export type WeaknessEstimateRow = typeof weaknessEstimates.$inferInsert;
export type StateHistoryRow = typeof weaknessStateHistory.$inferInsert;
export type SeriesPointRow = typeof weaknessSeriesPoint.$inferInsert;
export type KpiSnapshotRow = typeof kpiSnapshot.$inferInsert;

export type SeedProfileOptions = {
  timeClass: TimeClass;
  /** Analysed games, each with a `game_metrics` row (default 0). */
  games?: number;
  estimates?: Partial<WeaknessEstimateRow>[];
  history?: Partial<StateHistoryRow>[];
  /** Series points; a point without `gameId` gets its own synthetic game at its `playedAt`. */
  points?: Partial<SeriesPointRow>[];
  kpis?: Partial<KpiSnapshotRow>[];
  /** A `report_runs.input` snapshot: one run row holding it. */
  input?: unknown;
  /** `move_features.difficulty_status` per position, on one synthetic game. */
  difficulty?: ("ok" | "pending_maia" | "not_applicable")[];
};

const DAY = 86_400_000;
const BASE = Date.UTC(2026, 5, 1, 12);

async function game(db: Db, accountId: string, timeClass: TimeClass, playedAt: Date) {
  const { gameId } = await seedSyntheticGame(db, accountId, {
    playedAt: playedAt.toISOString(),
    timeClass,
    result: "1-0",
  });
  return gameId;
}

/** Writes a synthetic profile for one (account, time class). */
export async function seedProfile(
  db: Db,
  accountId: string,
  opts: SeedProfileOptions,
): Promise<void> {
  await assertSeedable(db);
  const tc = opts.timeClass;

  for (let i = 0; i < (opts.games ?? 0); i++) {
    const playedAt = new Date(BASE + i * DAY);
    const gameId = await game(db, accountId, tc, playedAt);
    await db.insert(gameMetrics).values({
      gameId,
      accountId,
      timeClass: tc,
      playedAt,
      protocol: "test-1",
      metrics: { version: 1 },
    });
  }

  for (const e of opts.estimates ?? []) {
    await db.insert(weaknessEstimates).values({
      seriesKey: "W1",
      k: 10,
      n: 200,
      nUnweighted: 2000,
      kUnweighted: 40,
      alpha: 11,
      beta: 191,
      mean: 0.05,
      ciLow: 0.04,
      ciHigh: 0.06,
      bandMedian: 0.03,
      bandP40: 0.025,
      confidence: 0.9,
      priority: 1,
      state: "active",
      ...e,
      accountId,
      timeClass: tc,
    });
  }

  for (const h of opts.history ?? []) {
    await db.insert(weaknessStateHistory).values({
      seriesKey: "W1",
      kind: "transition",
      toState: "active",
      evidence: {},
      detectorVersion: "test-1",
      protocol: "test-1",
      ...h,
      accountId,
      timeClass: tc,
    });
  }

  for (const p of opts.points ?? []) {
    const playedAt = p.playedAt ?? new Date(BASE);
    const gameId = p.gameId ?? (await game(db, accountId, tc, playedAt));
    await db.insert(weaknessSeriesPoint).values({
      seriesKey: "W1",
      events: 0,
      opportunities: 0,
      epl: 0,
      protocol: "test-1",
      ...p,
      playedAt,
      gameId,
      accountId,
      timeClass: tc,
    });
  }

  for (const k of opts.kpis ?? []) {
    const windowKey = k.windowKey ?? "last50";
    await db.insert(kpiSnapshot).values({
      metric: "accuracy",
      windowKind: windowKey.startsWith("last") ? "last_n" : "week",
      n: 0,
      protocol: "test-1",
      ...k,
      windowKey,
      accountId,
      timeClass: tc,
    });
  }

  if (opts.input !== undefined) {
    await db.insert(reportRuns).values({
      accountId,
      timeClass: tc,
      trigger: "manual",
      plan: "free",
      status: "done",
      promptVersion: "report@1",
      input: opts.input,
      isFirstReport: false,
    });
  }

  if (opts.difficulty && opts.difficulty.length > 0) {
    const gameId = await game(db, accountId, tc, new Date(BASE));
    await db.insert(moveFeatures).values(
      opts.difficulty.map((status, i) => ({
        gameId,
        ply: i + 1,
        enrichmentProtocol: "test-1",
        difficultyStatus: status,
        ...(status === "ok" ? { pBest: 0.5, pOk: 0.8, difficultyModel: "maia-test" } : {}),
      })),
    );
  }
}
