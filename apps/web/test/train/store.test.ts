import { drillItems, gameMetrics, moves, positionMultipv } from "@ci/db/generated";
import { eq, sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { type CoachScope, resolveCoachScope } from "../../src/server/profile";
import { isHeldOut } from "../../src/server/train/model";
import {
  loadTrainPage,
  practiseCount,
  recordAttempt,
  refreshItems,
} from "../../src/server/train/store";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// The trainer's rows against seeded positions: which become items, per-user isolation, the
// holdout, grading against the stored accepted set and the schedule after an attempt.

const db = webTestDb();
let userId: string;
let accountId: string;
let gameId: string;

const NOW = new Date("2026-11-23T10:00:00Z");
const MATE_FEN = "6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1";
const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const FREE_FEN = "4k3/8/8/3q4/8/2N5/8/4K3 w - - 0 1";

type Pos = {
  ply: number;
  fen: string;
  uci: string;
  best: string | null;
  before: number;
  after: number;
  judgement: "blunder" | null;
};

const POSITIONS: Pos[] = [
  // A missed back-rank mate (W4.missed and a blunder).
  { ply: 21, fen: MATE_FEN, uci: "d1d7", best: "d1d8", before: 99, after: 60, judgement: "blunder" },
  // A blunder in an already decided position: dropped.
  { ply: 23, fen: MATE_FEN, uci: "g1f1", best: "d1d8", before: 99, after: 95, judgement: "blunder" },
  // A plain blunder, with stored alternatives.
  { ply: 1, fen: START_FEN, uci: "f2f3", best: "e2e4", before: 50, after: 20, judgement: "blunder" },
  // A free queen capture missed: dropped.
  { ply: 3, fen: FREE_FEN, uci: "e1e2", best: "c3d5", before: 60, after: 30, judgement: "blunder" },
  // Not a blunder and no exemplar: never a candidate.
  { ply: 5, fen: START_FEN, uci: "a2a3", best: "e2e4", before: 50, after: 45, judgement: null },
];

async function scopeOf(uid: string): Promise<CoachScope> {
  const scope = await resolveCoachScope(db, uid, {});
  if (!scope) throw new Error("no scope");
  return scope;
}

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await db.execute(sql`delete from position_multipv`);
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
  ({ gameId } = await seedSyntheticGame(db, accountId, {
    playedAt: "2026-10-01T12:00:00Z",
    timeClass: "rapid",
    result: "0-1",
    playerColor: "white",
    noMoves: true,
  }));
  await db.insert(moves).values(
    POSITIONS.map((p) => ({
      gameId,
      ply: p.ply,
      color: "white" as const,
      san: "x",
      uci: p.uci,
      fenBefore: p.fen,
      bestUci: p.best,
      winBefore: p.before,
      winAfter: p.after,
      deltaW: p.before - p.after,
      accuracy: 50,
      judgement: p.judgement,
      phase: "middlegame" as const,
      decided: false,
    })),
  );
  await db.insert(gameMetrics).values({
    gameId,
    accountId,
    timeClass: "rapid",
    playedAt: new Date("2026-10-01T12:00:00Z"),
    protocol: "test-1",
    metrics: {
      version: 1,
      weakness: { "W4.missed": { events: 1, opportunities: 2, exemplarPlies: [21] } },
    },
  });
  await db.insert(positionMultipv).values({
    fenKey: START_FEN.split(" ").slice(0, 4).join(" "),
    protocol: "test-mpv",
    lines: [
      { uci: "e2e4", score: { cp: 30 }, pv: ["e2e4"] },
      { uci: "d2d4", score: { cp: 25 }, pv: ["d2d4"] },
      { uci: "g1f3", score: { cp: -200 }, pv: ["g1f3"] },
    ],
  });
});

const itemsOf = (uid: string) =>
  db.select().from(drillItems).where(eq(drillItems.userId, uid)).orderBy(drillItems.ply);

/** Makes every item of the user drillable (the holdout is tested on its own). */
const noHoldout = (uid: string) =>
  db.update(drillItems).set({ heldOut: false }).where(eq(drillItems.userId, uid));

describe("refreshItems", () => {
  it("keeps the missed mate and the blunder, and drops decided and free-capture positions", async () => {
    expect(await refreshItems(db, userId, await scopeOf(userId))).toBe(2);
    const items = await itemsOf(userId);
    expect(items.map((i) => [i.ply, i.sourceKey, i.sourceKeys])).toEqual([
      [1, "blunder", ["blunder"]],
      [21, "W4.missed", ["W4.missed", "blunder"]],
    ]);
    expect(items[0]?.acceptedUci).toEqual(["e2e4", "d2d4"]);
    expect(items[1]?.acceptedUci).toEqual(["d1d8"]);
    expect(items[1]).toMatchObject({ timeClass: "rapid", accountId, dueOn: null });
    for (const i of items) expect(i.heldOut).toBe(isHeldOut(i.gameId, i.ply));
    // A second visit adds nothing and rewrites nothing.
    expect(await refreshItems(db, userId, await scopeOf(userId))).toBe(0);
  });
});

describe("per-user isolation", () => {
  it("gives each user linked to an account their own items, and nobody else's", async () => {
    const other = (await createTestSession(db)).user.id;
    await db.execute(
      sql`insert into account_links (user_id, linked_account_id, method) values (${other}, ${accountId}, 'username')`,
    );
    await refreshItems(db, userId, await scopeOf(userId));
    await refreshItems(db, other, await scopeOf(other));
    const mine = await itemsOf(userId);
    const theirs = await itemsOf(other);
    expect(theirs).toHaveLength(2);
    expect(theirs.map((i) => i.id)).not.toContain(mine[0]?.id);
    const item = mine[0];
    if (!item) throw new Error("no item");
    await noHoldout(userId);
    expect(
      await recordAttempt(db, other, { itemId: item.id, uci: "e2e4", confidence: "sure", ms: 1 }, NOW),
    ).toBeNull();
    await noHoldout(other);
    const page = await loadTrainPage(db, other, await scopeOf(other), { source: null, now: NOW });
    expect(page?.session.map((s) => s.id).sort()).toEqual(theirs.map((i) => i.id).sort());
  });

  it("makes no items for a scope that is not the user's", async () => {
    const stranger = (await createTestSession(db)).user.id;
    expect(await refreshItems(db, stranger, await scopeOf(userId))).toBe(0);
  });
});

describe("the holdout", () => {
  it("is never served or graded", async () => {
    await refreshItems(db, userId, await scopeOf(userId));
    await db.update(drillItems).set({ heldOut: true }).where(eq(drillItems.ply, 21));
    await db.update(drillItems).set({ heldOut: false }).where(eq(drillItems.ply, 1));
    const page = await loadTrainPage(db, userId, await scopeOf(userId), { source: null, now: NOW });
    const [mate] = (await itemsOf(userId)).filter((i) => i.ply === 21);
    expect(page?.session).toHaveLength(1);
    expect(page?.session[0]?.id).not.toBe(mate?.id);
    expect(page?.counts.heldOut).toBe(1);
    expect(
      await recordAttempt(
        db,
        userId,
        { itemId: mate?.id ?? "", uci: "d1d8", confidence: "sure", ms: null },
        NOW,
      ),
    ).toBeNull();
  });
});

describe("a session", () => {
  it("grades a right and a wrong answer, and both are due again tomorrow", async () => {
    await refreshItems(db, userId, await scopeOf(userId));
    await noHoldout(userId);
    const page = await loadTrainPage(db, userId, await scopeOf(userId), { source: null, now: NOW });
    expect(page?.session).toHaveLength(2);
    expect(page?.session.find((s) => s.fen === MATE_FEN)).toMatchObject({
      orientation: "white",
      label: "Move 11, White to move",
    });
    const [blunder, mate] = await itemsOf(userId);
    if (!blunder || !mate) throw new Error("no items");

    // Another of the engine's good moves passes.
    const right = await recordAttempt(
      db,
      userId,
      { itemId: blunder.id, uci: "d2d4", confidence: "sure", ms: 4000 },
      NOW,
    );
    expect(right).toMatchObject({
      correct: true,
      recorded: true,
      dueOn: "2026-11-24",
      feedback: "Right: d4 is one of the engine's good moves here (it chose e4).",
    });
    const wrong = await recordAttempt(
      db,
      userId,
      { itemId: mate.id, uci: "d1d7", confidence: "sure", ms: 9000 },
      NOW,
    );
    expect(wrong).toMatchObject({
      correct: false,
      feedback: "Not this time: the engine's move was Rd8#.",
      solution: { uci: "d1d8", san: "Rd8#" },
      dueOn: "2026-11-24",
    });
    expect(wrong?.line.map((m) => m.san)).toEqual(["Rd8#"]);

    // Graded again the same day, but the schedule stays.
    const again = await recordAttempt(
      db,
      userId,
      { itemId: mate.id, uci: "d1d8", confidence: "sure", ms: 1 },
      NOW,
    );
    expect(again).toMatchObject({ correct: true, recorded: false, dueOn: "2026-11-24" });

    const after = await loadTrainPage(db, userId, await scopeOf(userId), { source: null, now: NOW });
    expect(after).toMatchObject({ doneToday: 2, dueTomorrow: 2, session: [] });
    const tomorrow = await loadTrainPage(db, userId, await scopeOf(userId), {
      source: null,
      now: new Date("2026-11-24T08:00:00Z"),
    });
    expect(tomorrow?.session).toHaveLength(2);
  });

  it("refuses an illegal move", async () => {
    await refreshItems(db, userId, await scopeOf(userId));
    await noHoldout(userId);
    const [blunder] = await itemsOf(userId);
    expect(
      await recordAttempt(
        db,
        userId,
        { itemId: blunder?.id ?? "", uci: "e2e5", confidence: "guess", ms: null },
        NOW,
      ),
    ).toBeNull();
  });

  it("filters to one source, and counts that source's positions for the weakness page", async () => {
    await refreshItems(db, userId, await scopeOf(userId));
    await noHoldout(userId);
    const page = await loadTrainPage(db, userId, await scopeOf(userId), {
      source: "W4.missed",
      now: NOW,
    });
    expect(page?.session.map((s) => s.fen)).toEqual([MATE_FEN]);
    expect(await practiseCount(db, userId, await scopeOf(userId), "W4.missed")).toBe(1);
    expect(await practiseCount(db, userId, await scopeOf(userId), "blunder")).toBe(2);
    expect(await practiseCount(db, userId, await scopeOf(userId), "W7.traps")).toBe(0);
  });

  it("measures the source's rate before the first drill, and says it is too early", async () => {
    await refreshItems(db, userId, await scopeOf(userId));
    await noHoldout(userId);
    const [, mate] = await itemsOf(userId);
    await recordAttempt(
      db,
      userId,
      { itemId: mate?.id ?? "", uci: "d1d8", confidence: "sure", ms: null },
      NOW,
    );
    const page = await loadTrainPage(db, userId, await scopeOf(userId), { source: null, now: NOW });
    const row = page?.efficacy.rows.find((r) => r.source === "W4.missed");
    expect(row).toMatchObject({
      drilled: 1,
      before: { events: 1, chances: 2 },
      after: { chances: 0 },
      verdict: "too_early",
    });
  });
});
