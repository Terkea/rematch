import { sql } from "drizzle-orm";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import {
  BENCHMARK_BIAS_TEXT,
  type Benchmark,
  baselineGroupLabel,
  benchmarkGroupLabel,
  benchmarkLabel,
  type CoachScope,
  getBenchmark,
  MODEL_BASELINE_LABEL,
  notComparedText,
  peerHeadline,
  peerPooledBasis,
  peerRateText,
  resolveCoachScope,
} from "../../src/server/profile";
import { resetGameTables, seedAccount } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const peers: Benchmark = {
  source: "peers",
  platform: "chesscom",
  low: 650,
  high: 950,
  players: 12,
  games: 31,
  chances: { W5: 31 },
};

describe("benchmark label", () => {
  it("names the players faced, their platform, rating window and count", () => {
    expect(benchmarkLabel(peers)).toBe(
      "Compared with players you've faced on chess.com, rated about 650–950 (12 players)",
    );
    expect(benchmarkLabel({ ...peers, players: 1 })).toBe(
      "Compared with players you've faced on chess.com, rated about 650–950 (1 player)",
    );
    expect(benchmarkLabel({ ...peers, platform: "lichess" })).toContain("faced on lichess,");
  });

  it("states the bias: only their games against you", () => {
    expect(BENCHMARK_BIAS_TEXT).toBe(
      "Only their games against you count, so it is a rough guide, not a rating-band average.",
    );
  });

  it("gives rows a short group name", () => {
    expect(benchmarkGroupLabel(peers)).toBe("Players you've faced, rated about 650–950");
  });

  it("names a row's pooled rate with the raw chances it is pooled over", () => {
    const b: Benchmark = { ...peers, chances: { "W4.missed": 65, W9a: 1 } };
    expect(peerPooledBasis(b, "W4.missed")).toBe("pooled over 65 of their chances");
    expect(peerPooledBasis(b, "W9a")).toBe("pooled over 1 of their chances");
    expect(peerPooledBasis(b, "W1")).toBeUndefined();
    expect(peerRateText(b, "W4.missed", 0.0957)).toBe(
      "Players you've faced, rated about 650–950: 9.6% (pooled over 65 of their chances)",
    );
    expect(peerRateText(b, "W1", 0.1)).toBe("Players you've faced, rated about 650–950: 10.0%");
    expect(peerRateText({ source: "lichess_bands" }, "W4.missed", 0.1)).toBeNull();
    expect(peerPooledBasis(null, "W4.missed")).toBeUndefined();
  });

  // ADR 0101 (#10): W1 is compared with the players faced; W5 with what the evaluation
  // predicts from a winning position, never named as the players faced.
  it("compares W1 with the players faced and W5 with the evaluation", () => {
    expect(notComparedText(peers, "W1")).toMatch(/too few of their chances/);
    expect(baselineGroupLabel(peers, "W1")).toBe("Players you've faced, rated about 650–950");
    expect(baselineGroupLabel(peers, "W5")).toBe(MODEL_BASELINE_LABEL);
    expect(MODEL_BASELINE_LABEL).toBe("What the evaluation predicts");
    expect(peerRateText(peers, "W5", 0.1667)).toBe("What the evaluation predicts: 16.7%");
    expect(peerPooledBasis(peers, "W5")).toBeUndefined();
    expect(baselineGroupLabel(null, "W5")).toBeUndefined();
  });

  it("says nothing peer-specific for the legacy band reference or none", () => {
    expect(benchmarkLabel({ source: "lichess_bands" })).toBeNull();
    expect(benchmarkLabel(null)).toBeNull();
    expect(benchmarkGroupLabel(null)).toBeUndefined();
  });
});

const db = webTestDb();
let userId: string;
let accountId: string;

async function scopeOf(uid: string): Promise<CoachScope> {
  const scope = await resolveCoachScope(db, uid, {});
  if (!scope) throw new Error("no scope");
  return scope;
}

async function storeBenchmark(account: string, served: boolean) {
  await db.execute(sql`
    insert into account_benchmark (account_id, time_class, source, center_rating, rating_low,
      rating_high, n_players, n_games)
    values (${account}, 'rapid', 'peers', 800, 650, 950, 12, 31)`);
  await db.execute(sql`
    insert into account_benchmark_stat (account_id, time_class, metric, median, p25, p40, p75,
      n_players, n_games, low_sample, n_chances)
    values (${account}, 'rapid', 'weakness:W5', 0.3, 0.2, 0.25, 0.4, 12, 31, ${!served}, 31),
      (${account}, 'rapid', 'accuracy', 70, 60, 65, 80, 12, 31, ${!served}, null)`);
}

describe("getBenchmark", () => {
  beforeAll(async () => {
    await resetAuthTables(db);
  });

  beforeEach(async () => {
    await resetAuthTables(db);
    await resetGameTables(db);
    userId = (await createTestSession(db)).user.id;
    accountId = (await seedAccount(db, { userId, platform: "chesscom", username: "Hero" }))
      .accountId;
    await seedProfile(db, accountId, { timeClass: "rapid", games: 1, estimates: [] });
  });

  it("reads the stored peer band of the scope", async () => {
    await storeBenchmark(accountId, true);
    expect(await getBenchmark(db, userId, await scopeOf(userId))).toEqual(peers);
  });

  it("counts the chances of the Both row, never a colour row (ADR 0082)", async () => {
    await storeBenchmark(accountId, true);
    // Colour rows for the same metric, with their own (smaller) chance counts.
    await db.execute(sql`
      insert into account_benchmark_stat (account_id, time_class, side, metric, median, p25, p40,
        p75, n_players, n_games, low_sample, n_chances)
      values (${accountId}, 'rapid', 'white', 'weakness:W5', 0.2, 0.1, 0.15, 0.3, 6, 15, false, 14),
        (${accountId}, 'rapid', 'black', 'weakness:W5', 0.4, 0.3, 0.35, 0.5, 6, 16, false, 17)`);
    const b = await getBenchmark(db, userId, await scopeOf(userId));
    expect(b && b.source === "peers" ? b.chances : null).toEqual({ W5: 31 });
  });

  // Audit A11: "(12 players)" is the both count; a colour names its own opponents and games.
  it("counts one colour's own players and games", async () => {
    await storeBenchmark(accountId, true);
    await db.execute(sql`
      insert into account_benchmark_stat (account_id, time_class, side, metric, median, p25, p40,
        p75, n_players, n_games, low_sample, n_chances)
      values (${accountId}, 'rapid', 'white', 'weakness:W5', 0.2, 0.1, 0.15, 0.3, 5, 12, false, 14),
        (${accountId}, 'rapid', 'white', 'accuracy', 70, 60, 65, 80, 6, 15, false, null),
        (${accountId}, 'rapid', 'black', 'accuracy', 70, 60, 65, 80, 7, 16, false, null)`);
    const b = await getBenchmark(db, userId, { ...(await scopeOf(userId)), side: "white" });
    expect(b).toMatchObject({ source: "peers", players: 6, games: 15 });
    const both = await getBenchmark(db, userId, await scopeOf(userId));
    expect(both).toMatchObject({ players: 12, games: 31 });
  });

  it("is null without a stored benchmark or without a served row", async () => {
    expect(await getBenchmark(db, userId, await scopeOf(userId))).toBeNull();
    await storeBenchmark(accountId, false);
    expect(await getBenchmark(db, userId, await scopeOf(userId))).toBeNull();
  });

  it("never reads another user's account", async () => {
    await storeBenchmark(accountId, true);
    const scope = await scopeOf(userId);
    const other = (await createTestSession(db)).user.id;
    expect(await getBenchmark(db, other, scope)).toBeNull();
  });
});

// Q1: the overview headline compares blunders a game with the players faced.
describe("peerHeadline", () => {
  beforeEach(async () => {
    await resetAuthTables(db);
    await resetGameTables(db);
    userId = (await createTestSession(db)).user.id;
    accountId = (await seedAccount(db, { userId, platform: "chesscom", username: "Hero" }))
      .accountId;
    await seedProfile(db, accountId, { timeClass: "rapid", games: 1, estimates: [] });
  });

  it("gives the band and the opponents' median blunders a game, Both rows only", async () => {
    await storeBenchmark(accountId, true);
    const scope = await scopeOf(userId);
    expect(await peerHeadline(db, userId, scope)).toMatchObject({ blunders: null, players: 12 });
    await db.execute(sql`
      insert into account_benchmark_stat (account_id, time_class, side, metric, median, p25, p40,
        p75, n_players, n_games, low_sample)
      values (${accountId}, 'rapid', 'both', 'blunders_per_game', 1.4, 1, 1.2, 2, 12, 31, false),
        (${accountId}, 'rapid', 'white', 'blunders_per_game', 0.9, 1, 1.2, 2, 6, 15, false)`);
    expect(await peerHeadline(db, userId, scope)).toEqual({
      platform: "chesscom",
      low: 650,
      high: 950,
      players: 12,
      blunders: 1.4,
    });
  });

  it("is null without a peer benchmark", async () => {
    expect(await peerHeadline(db, userId, await scopeOf(userId))).toBeNull();
  });
});
