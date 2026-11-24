import { drillItems, gameMetrics, moves, positionMultipv } from "@ci/db/generated";
import { eq, sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { ALGORITHM_ID } from "../../src/server/algorithm";
import { type CoachScope, resolveCoachScope } from "../../src/server/profile";
import { isHeldOut } from "../../src/server/train/model";
import {
  loadPractiseToday,
  loadTrainPage,
  MAX_CANDIDATES,
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
  {
    ply: 21,
    fen: MATE_FEN,
    uci: "d1d7",
    best: "d1d8",
    before: 99,
    after: 60,
    judgement: "blunder",
  },
  // A blunder in an already decided position: dropped.
  {
    ply: 23,
    fen: MATE_FEN,
    uci: "g1f1",
    best: "d1d8",
    before: 99,
    after: 95,
    judgement: "blunder",
  },
  // A plain blunder, with stored alternatives.
  {
    ply: 1,
    fen: START_FEN,
    uci: "f2f3",
    best: "e2e4",
    before: 50,
    after: 20,
    judgement: "blunder",
  },
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
    // ADR 0105: every item names the algorithm that chose it.
    for (const i of items) expect(i.algorithmId).toBe(ALGORITHM_ID);
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
      await recordAttempt(
        db,
        other,
        { itemId: item.id, uci: "e2e4", confidence: "sure", ms: 1 },
        NOW,
      ),
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
      feedback: "That works: d4 is one of the engine's good moves here (it chose e4).",
    });
    const wrong = await recordAttempt(
      db,
      userId,
      { itemId: mate.id, uci: "d1d7", confidence: "sure", ms: 9000 },
      NOW,
    );
    expect(wrong).toMatchObject({
      correct: false,
      feedback: "Not quite. The engine's move was Rd8#. Let's try this one again tomorrow.",
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

    const after = await loadTrainPage(db, userId, await scopeOf(userId), {
      source: null,
      now: NOW,
    });
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

describe("loadPractiseToday", () => {
  it("is a first run when the games hold no position yet, then counts today's session", async () => {
    const scope = await scopeOf(userId);
    const o = { source: null, now: NOW };
    const kept = await db.select().from(moves).where(eq(moves.gameId, gameId));
    await db.delete(moves).where(eq(moves.gameId, gameId));
    expect(await loadPractiseToday(db, userId, scope, o)).toEqual({
      kind: "first-run",
      stale: true,
    });
    await db.insert(moves).values(kept);
    await refreshItems(db, userId, scope);
    await noHoldout(userId);
    expect(await loadPractiseToday(db, userId, scope, o)).toEqual({
      stale: false,
      kind: "due",
      count: 2,
      source: null,
    });
    expect(await loadPractiseToday(db, userId, scope, { source: "W4.missed", now: NOW })).toEqual({
      stale: false,
      kind: "due",
      count: 1,
      source: "W4.missed",
    });
  });

  it("is done for today after the session, with tomorrow next", async () => {
    const scope = await scopeOf(userId);
    await refreshItems(db, userId, scope);
    await noHoldout(userId);
    for (const it of await itemsOf(userId)) {
      const uci = it.fen === MATE_FEN ? "h2h3" : "a2a3";
      await recordAttempt(db, userId, { itemId: it.id, uci, confidence: "guess", ms: 1 }, NOW);
    }
    expect(await loadPractiseToday(db, userId, scope, { source: null, now: NOW })).toEqual({
      stale: false,
      kind: "done",
      next: "2026-11-24",
      doneToday: 2,
    });
  });

  it("reads nothing for a scope that is not the user's", async () => {
    const other = (await createTestSession(db)).user.id;
    expect(
      await loadPractiseToday(db, other, await scopeOf(userId), { source: null, now: NOW }),
    ).toBeNull();
  });
});

// ---- R2-7: reads never write; items are made by `refreshItems` (a POST), once per game set ----

const countItems = async () =>
  Number(
    (
      (await db.execute(sql`select count(*)::int as n from drill_items`)) as unknown as {
        n: number;
      }[]
    )[0]?.n,
  );

/** A new analysed rapid game with one plain blunder at `ply` (a new candidate position). */
async function newBlunderGame(playedAt: string, ply = 7): Promise<string> {
  const { gameId: g } = await seedSyntheticGame(db, accountId, {
    playedAt,
    timeClass: "rapid",
    result: "0-1",
    playerColor: "white",
    noMoves: true,
  });
  await db.insert(moves).values({
    gameId: g,
    ply,
    color: "white",
    san: "x",
    uci: "f2f3",
    fenBefore: START_FEN,
    bestUci: "e2e4",
    winBefore: 50,
    winAfter: 20,
    deltaW: 30,
    accuracy: 50,
    judgement: "blunder",
    phase: "middlegame",
    decided: false,
  });
  return g;
}

/** A move row with no analysis (an opponent's move, for the last-move tint). */
const plainMove = (g: string, ply: number, uci: string) => ({
  gameId: g,
  ply,
  color: "black" as const,
  san: "x",
  uci,
  fenBefore: START_FEN,
  winBefore: 50,
  winAfter: 50,
  deltaW: 0,
  accuracy: 90,
  phase: "middlegame" as const,
  decided: false,
});

describe("no writes on GET", () => {
  it("loadTrainPage, loadPractiseToday and practiseCount add no items", async () => {
    const scope = await scopeOf(userId);
    const page = await loadTrainPage(db, userId, scope, { source: null, now: NOW });
    await loadPractiseToday(db, userId, scope, { source: null, now: NOW });
    await practiseCount(db, userId, scope, "blunder");
    expect(await countItems()).toBe(0);
    // The page says new positions are waiting, so the trainer asks for them (a POST).
    expect(page?.stale).toBe(true);
    expect(await refreshItems(db, userId, scope)).toBe(2);
    expect((await loadTrainPage(db, userId, scope, { source: null, now: NOW }))?.stale).toBe(false);
  });

  it("makes items at most once per set of analysed games", async () => {
    const scope = await scopeOf(userId);
    expect(await refreshItems(db, userId, scope)).toBe(2);
    // The same games: the refresh is skipped, even when items were removed under it.
    await db.delete(drillItems).where(eq(drillItems.userId, userId));
    expect(await refreshItems(db, userId, scope)).toBe(0);
    // A new analysed game: a new set, so the refresh runs again (and remakes every position).
    await newBlunderGame("2026-11-01T12:00:00Z");
    expect((await loadTrainPage(db, userId, scope, { source: null, now: NOW }))?.stale).toBe(true);
    expect(await refreshItems(db, userId, scope)).toBe(3);
    expect(await refreshItems(db, userId, scope)).toBe(0);
  });

  it("pages through more candidates than one batch", async () => {
    const scope = await scopeOf(userId);
    const { gameId: g } = await seedSyntheticGame(db, accountId, {
      playedAt: "2026-11-02T12:00:00Z",
      timeClass: "rapid",
      result: "0-1",
      playerColor: "white",
      noMoves: true,
    });
    const n = MAX_CANDIDATES + 5;
    await db.insert(moves).values(
      Array.from({ length: n }, (_, i) => ({
        gameId: g,
        ply: 2 * i + 1,
        color: "white" as const,
        san: "x",
        uci: "f2f3",
        fenBefore: START_FEN,
        bestUci: "e2e4",
        winBefore: 50,
        winAfter: 20,
        deltaW: 30,
        accuracy: 50,
        judgement: "blunder" as const,
        phase: "middlegame" as const,
        decided: false,
      })),
    );
    expect(await refreshItems(db, userId, scope)).toBe(n + 2);
  });
});

describe("the overview's count after new games (R2-7)", () => {
  it("counts the positions a refresh would add, without adding them, as the trainer will", async () => {
    const scope = await scopeOf(userId);
    await refreshItems(db, userId, scope);
    await noHoldout(userId);
    const before = await loadPractiseToday(db, userId, scope, { source: null, now: NOW });
    expect(before).toMatchObject({ kind: "due", count: 2, stale: false });
    // Three new games, each with a blunder outside the holdout.
    let made = 0;
    for (let ply = 7; made < 3; ply += 2) {
      const probe = await newBlunderGame(`2026-11-0${made + 1}T12:00:00Z`, ply);
      if (isHeldOut(probe, ply)) await db.execute(sql`delete from games where id = ${probe}`);
      else made += 1;
    }
    const n = await countItems();
    const practise = await loadPractiseToday(db, userId, scope, { source: null, now: NOW });
    expect(practise).toMatchObject({ kind: "due", count: 5, stale: true });
    expect(await practiseCount(db, userId, scope, "blunder")).toBe(5);
    expect(await countItems()).toBe(n);
    // The trainer's POST makes them; its session is the count the overview showed.
    await refreshItems(db, userId, scope);
    const page = await loadTrainPage(db, userId, scope, { source: null, now: NOW });
    expect(page?.session).toHaveLength(5);
    expect(await loadPractiseToday(db, userId, scope, { source: null, now: NOW })).toMatchObject({
      kind: "due",
      count: 5,
      stale: false,
    });
  });
});

// ---- R2-9: the schedule at store level ----

/** `n` drillable items on the seed game, introduced and due on `due` (or new, for null). */
async function seedItems(n: number, due: string | null): Promise<string[]> {
  const rows = await db
    .insert(drillItems)
    .values(
      Array.from({ length: n }, (_, i) => ({
        userId,
        accountId,
        timeClass: "rapid" as const,
        gameId,
        ply: 101 + 2 * i,
        sourceKey: "blunder",
        sourceKeys: ["blunder"],
        fen: START_FEN,
        playedUci: "f2f3",
        solutionUci: "e2e4",
        solutionLine: ["e2e4"],
        acceptedUci: ["e2e4"],
        winBefore: 50,
        winAfter: 20,
        tags: [],
        heldOut: false,
        dueOn: due,
        introducedOn: due === null ? null : "2026-11-01",
        lastAttemptOn: due === null ? null : "2026-11-01",
      })),
    )
    .returning({ id: drillItems.id });
  return rows.map((r) => r.id);
}

const attempt = (itemId: string, now: Date, uci = "e2e4", timeZone?: string) =>
  recordAttempt(db, userId, { itemId, uci, confidence: "sure", ms: 1 }, now, timeZone);

const attemptsOf = async (itemId: string) =>
  Number(
    (
      (await db.execute(
        sql`select count(*)::int as n from drill_attempts where item_id = ${itemId}`,
      )) as unknown as { n: number }[]
    )[0]?.n,
  );

const dayN = (n: number) => new Date(Date.parse("2026-11-01T10:00:00Z") + n * 86_400_000);

describe("retirement", () => {
  it("retires an item after three sure right answers a week or more apart", async () => {
    const [id] = await seedItems(1, null);
    if (!id) throw new Error("no item");
    expect(await attempt(id, dayN(0))).toMatchObject({ recorded: true, retired: false });
    expect(await attempt(id, dayN(7))).toMatchObject({ retired: false });
    expect(await attempt(id, dayN(14))).toMatchObject({ retired: false });
    expect(await attempt(id, dayN(21))).toMatchObject({ recorded: true, retired: true });
    const [row] = await db.select().from(drillItems).where(eq(drillItems.id, id));
    expect(row?.cleanPasses).toBe(3);
    expect(row?.retiredAt?.toISOString()).toBe(dayN(21).toISOString());
    // Retired: never graded or served again.
    expect(await attempt(id, dayN(30))).toBeNull();
    const page = await loadTrainPage(db, userId, await scopeOf(userId), {
      source: null,
      now: dayN(30),
    });
    expect(page?.counts.retired).toBe(1);
    expect(page?.session).toEqual([]);
  });

  it("a guess or a short gap is not a clean pass", async () => {
    const [id] = await seedItems(1, null);
    if (!id) throw new Error("no item");
    await attempt(id, dayN(0));
    await attempt(id, dayN(6));
    await recordAttempt(
      db,
      userId,
      { itemId: id, uci: "e2e4", confidence: "guess", ms: 1 },
      dayN(13),
    );
    const [row] = await db.select().from(drillItems).where(eq(drillItems.id, id));
    expect(row).toMatchObject({ cleanPasses: 0, streak: 3, retiredAt: null });
  });
});

describe("the daily cap", () => {
  it("serves at most 24 attempts a day, in sessions of 12", async () => {
    await seedItems(30, "2026-11-20");
    const scope = await scopeOf(userId);
    const load = () => loadTrainPage(db, userId, scope, { source: null, now: NOW });
    const first = await load();
    expect(first?.session).toHaveLength(12);
    for (const s of first?.session ?? []) await attempt(s.id, NOW);
    const second = await load();
    expect(second?.session).toHaveLength(12);
    expect(second?.session.map((s) => s.id)).not.toContain(first?.session[0]?.id);
    for (const s of second?.session ?? []) await attempt(s.id, NOW);
    expect(await load()).toMatchObject({ doneToday: 24, rightToday: 24, session: [] });
    expect(await loadPractiseToday(db, userId, scope, { source: null, now: NOW })).toMatchObject({
      kind: "done",
      next: "2026-11-24",
      doneToday: 24,
    });
  });
});

describe("one recorded attempt a day", () => {
  it("grades a same-day second attempt without recording it or moving the schedule", async () => {
    const [id] = await seedItems(1, "2026-11-20");
    if (!id) throw new Error("no item");
    expect(await attempt(id, NOW, "f2f3")).toMatchObject({ correct: false, recorded: true });
    const [after1] = await db.select().from(drillItems).where(eq(drillItems.id, id));
    expect(await attempt(id, new Date(NOW.getTime() + 3_600_000))).toMatchObject({
      correct: true,
      recorded: false,
      dueOn: "2026-11-24",
    });
    const [after2] = await db.select().from(drillItems).where(eq(drillItems.id, id));
    expect(after2).toEqual(after1);
    expect(await attemptsOf(id)).toBe(1);
  });

  it("records exactly one of two concurrent same-day attempts", async () => {
    const [id] = await seedItems(1, "2026-11-20");
    if (!id) throw new Error("no item");
    const results = await Promise.all([attempt(id, NOW), attempt(id, NOW, "f2f3")]);
    expect(results.map((r) => r?.recorded).sort()).toEqual([false, true]);
    expect(await attemptsOf(id)).toBe(1);
    const [row] = await db.select().from(drillItems).where(eq(drillItems.id, id));
    expect(row?.lastAttemptOn).toBe("2026-11-23");
  });
});

describe("the player's day (R2-8)", () => {
  const LA = "America/Los_Angeles";

  it("schedules and counts by the player's local day", async () => {
    // Only the seeded item: the game holds no other position.
    await db.delete(moves).where(eq(moves.gameId, gameId));
    const [id] = await seedItems(1, "2026-11-20");
    if (!id) throw new Error("no item");
    // 21:00 on 23 November in Los Angeles (05:00 UTC on the 24th).
    expect(await attempt(id, new Date("2026-11-24T05:00:00Z"), "e2e4", LA)).toMatchObject({
      recorded: true,
      dueOn: "2026-11-24",
    });
    const [row] = await db.select().from(drillItems).where(eq(drillItems.id, id));
    expect(row?.lastAttemptOn).toBe("2026-11-23");
    const scope = await scopeOf(userId);
    const at = (iso: string) => ({ source: null, now: new Date(iso), timeZone: LA });
    // 23:00 local: still the same day, so the attempt counts and the item is not due.
    expect(await loadTrainPage(db, userId, scope, at("2026-11-24T07:00:00Z"))).toMatchObject({
      doneToday: 1,
      session: [],
    });
    expect(await loadPractiseToday(db, userId, scope, at("2026-11-24T07:00:00Z"))).toMatchObject({
      kind: "done",
      doneToday: 1,
      next: "2026-11-24",
    });
    // A second attempt at 23:30 local is the same day: not recorded.
    expect(await attempt(id, new Date("2026-11-24T07:30:00Z"), "e2e4", LA)).toMatchObject({
      recorded: false,
    });
    // 01:00 local on the 24th: a new day, the item is due again, nothing done yet.
    const next = await loadTrainPage(db, userId, scope, at("2026-11-24T09:00:00Z"));
    expect(next?.doneToday).toBe(0);
    expect(next?.session.map((s) => s.id)).toEqual([id]);
    // In UTC the same instant is already the 24th's 09:00, and the 05:00 attempt is today's.
    expect(
      (await loadTrainPage(db, userId, scope, { source: null, now: new Date("2026-11-24T09:00Z") }))
        ?.doneToday,
    ).toBe(1);
  });
});

// ---- R2-7: bounded queries give the same results ----

describe("the efficacy panel and the last move, aggregated in SQL", () => {
  it("splits each source's rate at its first drill and compares drilled and held-back motifs", async () => {
    const scope = await scopeOf(userId);
    await refreshItems(db, userId, scope);
    await noHoldout(userId);
    const [blunder, mate] = await itemsOf(userId);
    if (!blunder || !mate) throw new Error("no items");
    await db
      .update(drillItems)
      .set({ tags: ["fork"] })
      .where(eq(drillItems.id, blunder.id));
    // A held-back item tagged with another motif (and the drilled one, which does not count), and
    // with `pin`, which EXP-0007 hid (ADR 0112): its counts never enter a pattern rate.
    const [held] = await seedItems(1, null);
    await db
      .update(drillItems)
      .set({ heldOut: true, tags: ["discoveredAttack", "pin", "fork"] })
      .where(eq(drillItems.id, held ?? ""));
    // A game before the first drill and two after it, with weakness and motif counts.
    const metricsGame = async (at: string, metrics: Record<string, unknown>, blunders: number) => {
      const { gameId: g } = await seedSyntheticGame(db, accountId, {
        playedAt: at,
        timeClass: "rapid",
        result: "1-0",
        playerColor: "white",
        noMoves: true,
      });
      await db.insert(moves).values(
        Array.from({ length: 4 }, (_, i) => ({
          gameId: g,
          ply: 2 * i + 1,
          color: "white" as const,
          san: "x",
          uci: "a2a3",
          fenBefore: START_FEN,
          bestUci: "a2a3",
          winBefore: 50,
          winAfter: 50,
          deltaW: 0,
          accuracy: 90,
          judgement: i < blunders ? ("blunder" as const) : null,
          phase: "middlegame" as const,
          decided: false,
        })),
      );
      await db.insert(gameMetrics).values({
        gameId: g,
        accountId,
        timeClass: "rapid",
        playedAt: new Date(at),
        protocol: "test-1",
        metrics: { version: 1, ...metrics },
      });
    };
    await metricsGame(
      "2026-11-10T12:00:00Z",
      { weakness: { "W4.missed": { events: 3, opportunities: 5 } } },
      1,
    );
    await attempt(mate.id, new Date("2026-11-15T12:00:00Z"), "d1d8");
    await attempt(blunder.id, new Date("2026-11-16T12:00:00Z"));
    await metricsGame(
      "2026-11-20T12:00:00Z",
      {
        weakness: {
          "W4.missed": { events: 1, opportunities: 4 },
          "W3.deep": { events: 9, opportunities: 9 },
        },
        missedByMotif: {
          fork: { opportunities: 4, misses: 1 },
          discoveredAttack: { opportunities: 2, misses: 2 },
          pin: { opportunities: 6, misses: 1 },
          skewer: { opportunities: 5, misses: 0 },
        },
      },
      2,
    );
    await metricsGame(
      "2026-11-21T12:00:00Z",
      {
        weakness: { "W4.missed": { events: 0, opportunities: 0 } },
        missedByMotif: { fork: { opportunities: 3, misses: 5 }, pin: "junk" },
      },
      0,
    );
    const page = await loadTrainPage(db, userId, scope, { source: null, now: NOW });
    const row = (s: string) => page?.efficacy.rows.find((r) => r.source === s);
    // W4.missed: first drill 15 Nov. Before: the seed game (1/2) and 10 Nov (3/5); after: 1/4.
    expect(row("W4.missed")).toMatchObject({
      drilled: 1,
      before: { events: 4, chances: 7 },
      after: { events: 1, chances: 4 },
    });
    expect(row("W4.missed")?.firstDrill?.toISOString()).toBe("2026-11-15T12:00:00.000Z");
    // Blunders: first drill 15 Nov (the mate is a blunder too). The seed game has 5 moves with 4
    // blunders and 10 Nov 4 moves with 1; after, 4 moves with 2 and 4 with none.
    expect(row("blunder")).toMatchObject({
      drilled: 2,
      before: { events: 5, chances: 9 },
      after: { events: 2, chances: 8 },
    });
    // Drilled motif fork (3 of 4 found, then 0 of 3), against the held-back discovered attack only
    // (0 of 2); the hidden pin's 5 of 6 are left out.
    expect(page?.efficacy.patterns.since?.toISOString()).toBe("2026-11-15T12:00:00.000Z");
    expect(page?.efficacy.patterns.drilled).toMatchObject({ events: 3, chances: 7 });
    expect(page?.efficacy.patterns.heldOut).toMatchObject({ events: 0, chances: 2 });
  });

  it("tints the opponent's last move from the exact previous ply of each item's game", async () => {
    const scope = await scopeOf(userId);
    // The seed game's ply 21 follows its ply 20; the new game's ply 7 follows its ply 6.
    await db.insert(moves).values(plainMove(gameId, 20, "g8h8"));
    const g2 = await newBlunderGame("2026-11-01T12:00:00Z", 7);
    await db.insert(moves).values(plainMove(g2, 6, "e7e5"));
    // A decoy at the other game's previous ply, which an IN x IN lookup would also read.
    await db.insert(moves).values(plainMove(g2, 20, "a7a6"));
    await refreshItems(db, userId, scope);
    await noHoldout(userId);
    const page = await loadTrainPage(db, userId, scope, { source: null, now: NOW });
    const byPly = new Map((await itemsOf(userId)).map((i) => [`${i.gameId}#${i.ply}`, i.id]));
    const last = (key: string) => page?.session.find((s) => s.id === byPly.get(key))?.lastMove;
    expect(last(`${gameId}#21`)).toEqual({ from: "g8", to: "h8" });
    expect(last(`${g2}#7`)).toEqual({ from: "e7", to: "e5" });
    expect(last(`${gameId}#1`)).toBeNull();
  });
});
