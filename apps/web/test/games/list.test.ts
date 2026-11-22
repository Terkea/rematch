import { games as fixtures } from "@ci/ui/fixtures";
import { beforeAll, describe, expect, it } from "vitest";
import {
  type GameFilters,
  type GameListQuery,
  type GameRow,
  listGames,
  PAGE_SIZE,
  type TimeClass,
} from "../../src/server/games";
import {
  resetGameTables,
  type SyntheticGame,
  seedAccount,
  seedFixtureGame,
  seedSyntheticGame,
} from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

const NO_FILTERS: GameFilters = {
  timeClass: null,
  result: null,
  colour: null,
  from: null,
  to: null,
  accountId: null,
};

function query(
  over: Partial<Omit<GameListQuery, "filters">> & { filters?: Partial<GameFilters> } = {},
): GameListQuery {
  return {
    filters: { ...NO_FILTERS, ...over.filters },
    sort: over.sort ?? { key: "played" as const, dir: "desc" as const },
    page: over.page ?? 1,
  };
}

/** Every page of a query, concatenated. */
async function allRows(userId: string, q: GameListQuery): Promise<GameRow[]> {
  const out: GameRow[] = [];
  for (let page = 1; ; page++) {
    const p = await listGames(db, userId, { ...q, page });
    out.push(...p.rows);
    if (page >= p.pageCount) return out;
  }
}

type Seeded = SyntheticGame & { id: string; accountId: string };

let alice: string;
let bob: string;
let aliceMain: string;
let aliceAlt: string;
let bobAccount: string;
let blunderGameId: string;
const seeded: Seeded[] = [];
const bobIds: string[] = [];

const TIME_CLASSES: TimeClass[] = ["rapid", "blitz", "bullet"];
const RESULTS = ["1-0", "0-1", "1/2-1/2", "1-0", "*"] as const;

function synthetic(i: number): SyntheticGame {
  const playedAt =
    i === 28
      ? "2026-10-31T23:59:59Z"
      : i === 29
        ? "2026-11-01T00:00:00Z"
        : new Date(Date.UTC(2026, 9, 1 + i, 12)).toISOString();
  return {
    playedAt,
    timeClass: TIME_CLASSES[i % 3] as TimeClass,
    result: RESULTS[i % RESULTS.length] as SyntheticGame["result"],
    playerColor: i % 2 === 0 ? "white" : "black",
    accuracy: i % 7 === 3 ? null : 40 + ((i * 13) % 55),
    status: i % 10 === 9 ? "pending" : "done",
    counts: { blunders: i % 4, mistakes: i % 2, inaccuracies: (i * 5) % 3 },
    opponentRating: 1200 + ((i * 37) % 700),
  };
}

beforeAll(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  alice = (await createTestSession(db)).user.id;
  bob = (await createTestSession(db)).user.id;
  aliceMain = (await seedAccount(db, { userId: alice, username: "AliceMain", verified: true }))
    .accountId;
  aliceAlt = (await seedAccount(db, { userId: alice, username: "AliceAlt" })).accountId;
  bobAccount = (await seedAccount(db, { userId: bob, username: "BobOnly", verified: true }))
    .accountId;

  for (let i = 0; i < 30; i++) {
    const row = synthetic(i);
    const accountId = i < 20 ? aliceMain : aliceAlt;
    const { gameId } = await seedSyntheticGame(db, accountId, row);
    seeded.push({ ...row, id: gameId, accountId });
  }
  blunderGameId = (
    await seedFixtureGame(db, aliceMain, fixtures.middlegameBlunder, {
      playedAt: new Date("2026-09-10T12:00:00Z"),
    })
  ).gameId;
  await seedFixtureGame(db, aliceMain, fixtures.endgameGrind, {
    playedAt: new Date("2026-09-11T12:00:00Z"),
  });
  await seedFixtureGame(db, aliceAlt, fixtures.shortAttack, {
    playedAt: new Date("2026-09-12T12:00:00Z"),
  });

  // Bob's games share dates, classes and results with Alice's, so every filter would match them.
  for (let i = 0; i < 4; i++) {
    bobIds.push((await seedSyntheticGame(db, bobAccount, synthetic(i))).gameId);
  }
  bobIds.push(
    (
      await seedFixtureGame(db, bobAccount, fixtures.middlegameBlunder, {
        playedAt: new Date("2026-10-15T12:00:00Z"),
      })
    ).gameId,
  );
});

function wins(s: Seeded): "win" | "draw" | "loss" | "unfinished" {
  if (s.result === "*") return "unfinished";
  if (s.result === "1/2-1/2") return "draw";
  return (s.result === "1-0") === (s.playerColor === "white") ? "win" : "loss";
}

describe("listGames", () => {
  it("lists only the user's linked accounts", async () => {
    const rows = await allRows(alice, query());
    expect(rows).toHaveLength(33);
    for (const r of rows) expect([aliceMain, aliceAlt]).toContain(r.accountId);
    for (const id of bobIds) expect(rows.map((r) => r.id)).not.toContain(id);

    const foreign = await listGames(db, alice, query({ filters: { accountId: bobAccount } }));
    expect(foreign).toEqual({ rows: [], total: 0, page: 1, pageCount: 1 });

    const alt = await allRows(alice, query({ filters: { accountId: aliceAlt } }));
    expect(alt).toHaveLength(11);
    for (const r of alt) expect(r.accountId).toBe(aliceAlt);

    const bobsOwn = await allRows(bob, query());
    expect(bobsOwn.map((r) => r.id).sort()).toEqual([...bobIds].sort());

    const stranger = (await createTestSession(db)).user.id;
    expect((await listGames(db, stranger, query())).total).toBe(0);
  });

  it("filters by time class, result, colour and inclusive UTC dates", async () => {
    const f = { timeClass: "rapid" as const, result: "loss" as const, colour: "black" as const };
    const got = await allRows(alice, query({ filters: f }));
    const want = seeded.filter(
      (s) => s.timeClass === "rapid" && wins(s) === "loss" && s.playerColor === "black",
    );
    expect(want.length).toBeGreaterThan(0);
    expect(got.map((r) => r.id).sort()).toEqual(want.map((s) => s.id).sort());

    for (const result of ["win", "draw", "loss"] as const) {
      const rows = await allRows(alice, query({ filters: { result } }));
      expect(rows.length).toBeGreaterThan(0);
      for (const r of rows) expect(r.result).toBe(result);
    }

    const late = seeded[28] as Seeded;
    const midnight = seeded[29] as Seeded;
    const october = await allRows(
      alice,
      query({ filters: { from: "2026-10-01", to: "2026-10-31" } }),
    );
    expect(october.map((r) => r.id)).toContain(late.id);
    expect(october.map((r) => r.id)).not.toContain(midnight.id);
    expect(october).toHaveLength(29);

    const november = await allRows(alice, query({ filters: { from: "2026-11-01", to: null } }));
    expect(november.map((r) => r.id)).toEqual([midnight.id]);

    const firstDay = await allRows(
      alice,
      query({ filters: { from: "2026-10-01", to: "2026-10-01" } }),
    );
    expect(firstDay.map((r) => r.id)).toEqual([(seeded[0] as Seeded).id]);
  });

  it("sorts by accuracy with nulls last", async () => {
    for (const dir of ["desc", "asc"] as const) {
      const rows = await allRows(alice, query({ sort: { key: "accuracy", dir } }));
      expect(rows).toHaveLength(33);
      const values = rows.map((r) => r.accuracy);
      const firstNull = values.indexOf(null);
      expect(firstNull).toBeGreaterThan(0);
      expect(values.slice(firstNull).every((v) => v === null)).toBe(true);
      const known = values.slice(0, firstNull) as number[];
      const sorted = [...known].sort((a, b) => (dir === "asc" ? a - b : b - a));
      expect(known).toEqual(sorted);
    }
  });

  it("sorts by opponent rating and by blunders", async () => {
    const byOpp = await allRows(alice, query({ sort: { key: "opponent", dir: "desc" } }));
    const ratings = byOpp.map((r) => r.opponent.rating as number);
    expect(ratings).toEqual([...ratings].sort((a, b) => b - a));
    expect(ratings[0]).toBe(3029);

    const byBlunders = await allRows(alice, query({ sort: { key: "blunders", dir: "desc" } }));
    const counts = byBlunders.map((r) => r.counts?.blunders ?? null);
    const firstNull = counts.indexOf(null);
    const known = counts.slice(0, firstNull) as number[];
    expect(known).toEqual([...known].sort((a, b) => b - a));
    expect(counts.slice(firstNull).every((c) => c === null)).toBe(true);
    // Ties break on played_at desc.
    const tied = byBlunders.filter((r) => r.counts?.blunders === 3 && r.id !== blunderGameId);
    const times = tied.map((r) => r.playedAt.getTime());
    expect(times).toEqual([...times].sort((a, b) => b - a));
  });

  it("counts only the player's judgements", async () => {
    const rows = await allRows(alice, query());
    const row = rows.find((r) => r.id === blunderGameId) as GameRow;
    const fx = fixtures.middlegameBlunder;
    const mine = fx.plies.filter((p) => p.color === fx.playerColor);
    expect(row.counts).toEqual({
      blunders: mine.filter((p) => p.judgement === "blunder").length,
      mistakes: mine.filter((p) => p.judgement === "mistake").length,
      inaccuracies: mine.filter((p) => p.judgement === "inaccuracy").length,
    });
    expect(row.plies).toBe(fx.plies.length);
    expect(row.player).toEqual(fx.white);
    expect(row.opponent).toEqual(fx.black);
    expect(row.result).toBe("win");
    expect(row.accuracy).toBeGreaterThan(0);

    const s = seeded[6] as Seeded;
    const synth = rows.find((r) => r.id === s.id) as GameRow;
    expect(synth.counts).toEqual({ blunders: 2, mistakes: 0, inaccuracies: 0 });
  });

  it("pages by 25 and clamps past the end", async () => {
    const first = await listGames(db, alice, query());
    expect(first.rows).toHaveLength(PAGE_SIZE);
    expect(first).toMatchObject({ total: 33, page: 1, pageCount: 2 });
    const past = await listGames(db, alice, query({ page: 9 }));
    expect(past).toMatchObject({ total: 33, page: 2, pageCount: 2 });
    expect(past.rows).toHaveLength(8);
    const ids = new Set([...first.rows, ...past.rows].map((r) => r.id));
    expect(ids.size).toBe(33);
    const times = [...first.rows, ...past.rows].map((r) => r.playedAt.getTime());
    expect(times).toEqual([...times].sort((a, b) => b - a));
  });

  it("pending row has null accuracy and counts", async () => {
    const pending = seeded.filter((s) => s.status === "pending");
    expect(pending.length).toBe(3);
    const rows = await allRows(alice, query());
    for (const s of pending) {
      const row = rows.find((r) => r.id === s.id) as GameRow;
      expect(row.analysisStatus).toBe("pending");
      expect(row.accuracy).toBeNull();
      expect(row.counts).toBeNull();
    }
  });
});
