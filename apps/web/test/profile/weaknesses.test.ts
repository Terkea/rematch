import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import {
  type CoachScope,
  getWeakness,
  listWeaknesses,
  resolveCoachScope,
} from "../../src/server/profile";
import { wilson } from "../../src/server/stats";
import { resetGameTables, seedAccount } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
let userId: string;
let accountId: string;

const at = (iso: string) => new Date(`${iso}T12:00:00Z`);

async function scopeOf(uid: string): Promise<CoachScope> {
  const scope = await resolveCoachScope(db, uid, {});
  if (!scope) throw new Error("no scope");
  return scope;
}

beforeAll(async () => {
  await resetAuthTables(db);
});

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
});

describe("listWeaknesses", () => {
  it("lists by priority with stored intervals", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [
        { seriesKey: "W2", priority: 0.4, mean: 0.11, ciLow: 0.0912345, ciHigh: 0.1387654 },
        { seriesKey: "W1", priority: 2.5, mean: 0.05, ciLow: 0.0411111, ciHigh: 0.0622222 },
        { seriesKey: "W9b", priority: 0.4, mean: 0.2, ciLow: 0.15, ciHigh: 0.25 },
      ],
    });
    const list = await listWeaknesses(db, userId, await scopeOf(userId));
    expect(list.map((w) => w.seriesKey)).toEqual(["W1", "W2", "W9b"]);
    expect(list[0]?.rate).toEqual({ mean: 0.05, low: 0.0411111, high: 0.0622222, level: 0.8 });
    expect(list[1]?.rate).toEqual({ mean: 0.11, low: 0.0912345, high: 0.1387654, level: 0.8 });
    expect(list[0]).toMatchObject({
      contractId: "W1",
      label: "Hung material",
      area: "calculation",
      state: "active",
      band: { median: 0.03, p40: 0.025 },
      onset: null,
    });
    expect(list[2]?.area).toBe("time");
  });

  it("insufficient weakness is labelled early", async () => {
    // Rust min_evidence(W1) is 1000 opportunities (the catalogue snapshot).
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [
        { seriesKey: "W1", nUnweighted: 120, kUnweighted: 6 },
        { seriesKey: "W5", nUnweighted: 15, kUnweighted: 2 },
      ],
    });
    const list = await listWeaknesses(db, userId, await scopeOf(userId));
    const w1 = list.find((w) => w.seriesKey === "W1");
    expect(w1?.evidence).toEqual({
      events: 6,
      chances: 120,
      minEvidence: 1000,
      sufficient: false,
      needMore: 880,
    });
    const w5 = list.find((w) => w.seriesKey === "W5");
    expect(w5?.evidence).toMatchObject({ minEvidence: 15, sufficient: true, needMore: null });
  });

  it("onset is the newest onset row", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      history: [
        { kind: "onset", toState: "emerging", onsetAt: at("2026-07-01") },
        {
          kind: "onset",
          toState: "active",
          onsetAt: at("2026-08-01"),
          onsetLow: at("2026-07-20"),
          onsetHigh: at("2026-08-10"),
        },
        { kind: "transition", toState: "active", atPlayedAt: at("2026-08-15") },
      ],
    });
    const [w] = await listWeaknesses(db, userId, await scopeOf(userId));
    expect(w?.onset).toEqual({
      at: at("2026-08-01"),
      low: at("2026-07-20"),
      high: at("2026-08-10"),
      gameId: null,
    });
  });

  it("another user's scope lists nothing", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
    });
    const mallory = (await createTestSession(db)).user.id;
    expect(await listWeaknesses(db, mallory, await scopeOf(userId))).toEqual([]);
  });
});

describe("getWeakness", () => {
  it("foreign series is not found", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
    });
    const bob = (await createTestSession(db)).user.id;
    const bobAccount = (await seedAccount(db, { userId: bob, username: "Bob" })).accountId;
    await seedProfile(db, bobAccount, { timeClass: "rapid", games: 1 });
    const aliceScope = await scopeOf(userId);
    expect(await getWeakness(db, userId, aliceScope, "W1")).not.toBeNull();
    // Bob's own scope has no W1, and Alice's scope is not his to read.
    expect(await getWeakness(db, bob, await scopeOf(bob), "W1")).toBeNull();
    expect(await getWeakness(db, bob, aliceScope, "W1")).toBeNull();
  });

  it("unknown series key is not found", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
    });
    const scope = await scopeOf(userId);
    expect(await getWeakness(db, userId, scope, "W99")).toBeNull();
    expect(await getWeakness(db, userId, scope, "x'; drop table games")).toBeNull();
  });

  it("timeline segments chain the transitions", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      history: [
        { kind: "transition", toState: "not_an_issue", atPlayedAt: at("2026-03-01") },
        {
          kind: "transition",
          fromState: "not_an_issue",
          toState: "emerging",
          atPlayedAt: at("2026-05-04"),
          pChange: 0.91,
          rateBefore: 0.03,
          rateAfter: 0.06,
        },
        {
          kind: "transition",
          fromState: "emerging",
          toState: "active",
          atPlayedAt: at("2026-06-10"),
        },
      ],
    });
    const w = await getWeakness(db, userId, await scopeOf(userId), "W1");
    expect(w?.timeline).toEqual({
      id: "W1",
      label: "Hung material",
      segments: [
        { state: "not_an_issue", from: "2026-03-01", to: "2026-05-04" },
        { state: "emerging", from: "2026-05-04", to: "2026-06-10" },
        { state: "active", from: "2026-06-10", to: null },
      ],
    });
    expect(w?.history).toHaveLength(3);
    expect(w?.history[1]).toMatchObject({
      kind: "transition",
      fromState: "not_an_issue",
      toState: "emerging",
      at: at("2026-05-04"),
      rateBefore: expect.closeTo(0.03, 6),
      causeHints: [],
    });
  });

  it("weekly rates use wilson and keep empty weeks null", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      points: [
        // ISO week 2026-W45 (2 to 8 Nov): 3 events in 60 chances over two games.
        { playedAt: at("2026-11-02"), events: 1, opportunities: 25 },
        { playedAt: at("2026-11-08"), events: 2, opportunities: 35 },
        // 2026-W46: a game with no chance.
        { playedAt: at("2026-11-10"), events: 0, opportunities: 0 },
        // Another series must not leak in.
        { seriesKey: "W2", playedAt: at("2026-11-03"), events: 9, opportunities: 9 },
      ],
    });
    const w = await getWeakness(db, userId, await scopeOf(userId), "W1");
    const interval = wilson(3, 60, 0.8);
    expect(w?.weekly).toEqual([
      { week: "2026-W45", events: 3, chances: 60, rate: { value: 3 / 60, ...interval } },
      { week: "2026-W46", events: 0, chances: 0, rate: null },
    ]);
  });

  it("weekly keeps only the newest 26 weeks", async () => {
    const points = Array.from({ length: 30 }, (_, i) => ({
      playedAt: new Date(Date.UTC(2026, 0, 5 + i * 7, 12)),
      events: 1,
      opportunities: 10,
    }));
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      points,
    });
    const w = await getWeakness(db, userId, await scopeOf(userId), "W1");
    expect(w?.weekly).toHaveLength(26);
    expect(w?.weekly[0]?.week).toBe("2026-W06");
    expect(w?.weekly[25]?.week).toBe("2026-W31");
  });

  it("exemplars come from the newest report input", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W3.shallow" }, { seriesKey: "W1" }],
    });
    const scope = await scopeOf(userId);
    expect((await getWeakness(db, userId, scope, "W1"))?.exemplars).toBeNull();

    const exemplar = (gameId: string) => ({
      gameId,
      ply: 23,
      playedAt: "2026-11-01T10:00:00.000Z",
      fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
      played: { san: "Nf6", uci: "g8f6" },
      best: { san: "e5", uci: "e7e5" },
      bestPv: ["e7e5", "g1f3"],
      refutationPv: ["e4e5"],
      tags: ["fork"],
      evalBefore: { cp: 20, mate: null },
      evalAfter: { cp: 40, mate: null },
      winBefore: 48,
      winAfter: 45,
    });
    const input = (tag: string) => ({
      weaknesses: [
        { id: "W3", variant: "deep", exemplars: [exemplar(`deep-${tag}`)] },
        { id: "W3", variant: "shallow", exemplars: [exemplar(`shallow-${tag}`)] },
      ],
    });
    await seedProfile(db, accountId, { timeClass: "rapid", input: input("old") });
    await seedProfile(db, accountId, { timeClass: "rapid", input: input("new") });

    const w = await getWeakness(db, userId, scope, "W3.shallow");
    expect(w?.exemplars?.fromReportAt).toBeInstanceOf(Date);
    expect(w?.exemplars?.items).toEqual([
      {
        gameId: "shallow-new",
        ply: 23,
        playedAt: "2026-11-01T10:00:00.000Z",
        fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        played: { san: "Nf6", uci: "g8f6" },
        best: { san: "e5", uci: "e7e5" },
        bestPv: ["e7e5", "g1f3"],
        refutationPv: ["e4e5"],
        tags: ["fork"],
        winBefore: 48,
        winAfter: 45,
      },
    ]);
    // A report exists, but it cited no W1 position.
    expect((await getWeakness(db, userId, scope, "W1"))?.exemplars?.items).toEqual([]);
  });
});
