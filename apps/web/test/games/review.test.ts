import { randomUUID } from "node:crypto";
import { winPercent } from "@ci/chess-core";
import { moves } from "@ci/db";
import { games as fixtures, type GameFixture } from "@ci/ui/fixtures";
import { Chess } from "chess.js";
import { and, eq } from "drizzle-orm";
import { beforeAll, describe, expect, it } from "vitest";
import { getGameReview, type ReviewGame, scoreFromWin, sourceUrl } from "../../src/server/games";
import { resetGameTables, seedAccount, seedFixtureGame, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

let alice: string;
let bob: string;
let aliceAccount: string;
let orphanAccount: string;

const blunder = fixtures.middlegameBlunder;

/** White's win% before each fixture ply (the start is level, as the seeder assumes). */
function winsBefore(f: GameFixture): number[] {
  return [50, ...f.plies.map((p) => p.winWhite)];
}

/** The player's judged plies, worst first by the mover's win% drop, ties by the lower ply. */
function expectedMoments(f: GameFixture): number[] {
  const before = winsBefore(f);
  return f.plies
    .filter((p) => p.color === f.playerColor && p.judgement != null)
    .map((p) => {
      const i = p.ply - 1;
      const pov = (w: number) => (p.color === "white" ? w : 100 - w);
      return { ply: p.ply, delta: Math.max(0, pov(before[i] as number) - pov(p.winWhite)) };
    })
    .sort((a, b) => b.delta - a.delta || a.ply - b.ply)
    .map((m) => m.ply);
}

async function review(userId: string, gameId: string): Promise<ReviewGame> {
  const r = await getGameReview(db, userId, gameId);
  if (!r) throw new Error("expected a review");
  return r;
}

beforeAll(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  alice = (await createTestSession(db)).user.id;
  bob = (await createTestSession(db)).user.id;
  aliceAccount = (await seedAccount(db, { userId: alice, username: "AliceReview", verified: true }))
    .accountId;
  await seedAccount(db, { userId: bob, username: "BobReview", verified: true });
  // An account nobody holds a link to, as after a user removes it.
  orphanAccount = (await seedAccount(db, { username: "Orphaned" })).accountId;
});

describe("getGameReview scope", () => {
  it("foreign game is not found", async () => {
    const { gameId } = await seedFixtureGame(db, aliceAccount, blunder, { protocol: "scope-1" });
    expect(await getGameReview(db, alice, gameId)).not.toBeNull();
    expect(await getGameReview(db, bob, gameId)).toBeNull();
  });

  it("a game of an unlinked account is not found", async () => {
    const { gameId } = await seedSyntheticGame(db, orphanAccount, {
      playedAt: "2026-10-01T12:00:00Z",
      timeClass: "rapid",
      result: "1-0",
    });
    expect(await getGameReview(db, alice, gameId)).toBeNull();
    expect(await getGameReview(db, bob, gameId)).toBeNull();
  });

  it("malformed id is not found", async () => {
    await expect(getGameReview(db, alice, "x'; drop")).resolves.toBeNull();
    await expect(getGameReview(db, alice, "")).resolves.toBeNull();
    await expect(getGameReview(db, alice, randomUUID())).resolves.toBeNull();
  });
});

describe("getGameReview analysed game", () => {
  let g: ReviewGame;
  beforeAll(async () => {
    const { gameId } = await seedFixtureGame(db, aliceAccount, blunder, {
      protocol: "review-blunder",
      termination: "Normal",
      baseSeconds: 600,
      incrementSeconds: 0,
    });
    g = await review(alice, gameId);
  });

  it("analysed fixture game maps every ply", () => {
    expect(g.analysed).toBe(true);
    expect(g.analysisStatus).toBe("done");
    expect(g.plies).toHaveLength(blunder.plies.length);
    expect(g.plies.map((p) => p.san)).toEqual(blunder.plies.map((p) => p.san));
    expect(g.plies.map((p) => p.uci)).toEqual(blunder.plies.map((p) => p.uci));
    expect(g.plies.map((p) => p.color)).toEqual(blunder.plies.map((p) => p.color));
    expect(g.plies.map((p) => p.fenAfter)).toEqual(blunder.plies.map((p) => p.fenAfter));
    expect(g.plies.map((p) => p.analysis?.judgement)).toEqual(
      blunder.plies.map((p) => p.judgement),
    );
    expect(g.plies.map((p) => p.analysis?.bestUci)).toEqual(blunder.plies.map((p) => p.bestUci));
    blunder.plies.forEach((p, i) => {
      const a = g.plies[i]?.analysis;
      // White's point of view on every ply, Black's included.
      expect(a?.winWhite).toBeCloseTo(p.winWhite, 9);
      expect(a?.scoreSource).toBe("engine");
      expect(a?.score).toEqual(p.mateWhite == null ? { cp: p.cpWhite } : { mate: p.mateWhite });
      expect(a?.phase).toBe(p.phase);
    });
  });

  it("Black's plies are flipped back to White's point of view", () => {
    const blackPly = blunder.plies.find(
      (p) => p.color === "black" && Math.abs(p.winWhite - 50) > 5,
    );
    if (!blackPly) throw new Error("fixture has no lopsided black ply");
    const a = g.plies[blackPly.ply - 1]?.analysis;
    expect(a?.winWhite).toBeCloseTo(blackPly.winWhite, 9);
    expect(a?.winWhite).not.toBeCloseTo(100 - blackPly.winWhite, 3);
  });

  it("carries the game's header and the player's colour", () => {
    expect(g.playerColor).toBe(blunder.playerColor);
    expect(g.white).toEqual(blunder.white);
    expect(g.black).toEqual(blunder.black);
    expect(g.result).toBe(blunder.result);
    expect(g.playerResult).toBe("win");
    expect(g.termination).toBe("Normal");
    expect(g.baseSeconds).toBe(600);
    expect(g.incrementSeconds).toBe(0);
    expect(g.eco).toBe(blunder.eco);
    expect(g.openingName).toBe(blunder.openingName);
    expect(g.platform).toBe("chesscom");
    // A chess.com game whose PGN has no Link header has no source link.
    expect(g.sourceUrl).toBeNull();
    expect(g.accuracy.white).toEqual(expect.any(Number));
    expect(g.accuracy.black).toEqual(expect.any(Number));
    expect(g.startFen).toBe(blunder.startFen);
    expect(g.startScore).toEqual({ score: { cp: 0 }, source: "engine" });
  });

  it("reads clocks and phases", () => {
    expect(g.hasClocks).toBe(true);
    expect(g.plies.map((p) => p.clockAfter)).toEqual(blunder.plies.map((p) => p.clockSeconds));
    expect(g.phases).toEqual({
      middlegameStartPly: blunder.plies.find((p) => p.phase === "middlegame")?.ply,
      endgameStartPly: blunder.plies.find((p) => p.phase === "endgame")?.ply,
    });
  });

  it("best classification", () => {
    const best = blunder.plies.filter((p) => p.uci === p.bestUci && p.judgement == null);
    expect(best.length).toBeGreaterThan(0);
    for (const p of blunder.plies) {
      const c = g.plies[p.ply - 1]?.analysis?.classification;
      if (p.judgement != null) expect(c).toBe(p.judgement);
      else if (p.uci === p.bestUci) expect(c).toBe("best");
      else expect(c).toBeNull();
    }
  });

  it("moments are the player's, worst first", () => {
    const want = expectedMoments(blunder);
    expect(want.length).toBeGreaterThan(1);
    expect(g.moments).toEqual(want);
    for (const ply of g.moments) expect(g.plies[ply - 1]?.color).toBe(g.playerColor);
  });
});

describe("getGameReview edge cases", () => {
  it("a Black player's moments are Black's plies", async () => {
    const f = fixtures.shortAttack;
    expect(f.playerColor).toBe("black");
    const { gameId } = await seedFixtureGame(db, aliceAccount, f, { protocol: "review-short" });
    const g = await review(alice, gameId);
    expect(g.playerColor).toBe("black");
    expect(g.playerResult).toBe("win");
    expect(g.moments).toEqual(expectedMoments(f));
    for (const ply of g.moments) expect(g.plies[ply - 1]?.color).toBe("black");
  });

  it("missing eval rows are derived", async () => {
    const { gameId } = await seedFixtureGame(db, aliceAccount, blunder, {
      protocol: "review-no-evals",
      withEvals: false,
    });
    const g = await review(alice, gameId);
    expect(g.analysed).toBe(true);
    for (const p of g.plies) {
      expect(p.analysis?.scoreSource).toBe("derived");
      expect(p.analysis?.score).toEqual(scoreFromWin(p.analysis?.winWhite as number));
    }
    expect(g.startScore).toEqual({ score: { cp: 0 }, source: "derived" });
    expect(scoreFromWin(50)).toEqual({ cp: 0 });
    expect(Object.is((scoreFromWin(50) as { cp: number }).cp, 0)).toBe(true);
    expect(scoreFromWin(100)).toEqual({ cp: 1000 });
    expect(scoreFromWin(0)).toEqual({ cp: -1000 });
    expect(scoreFromWin(winPercent({ cp: 250 }))).toEqual({ cp: 250 });
    expect(scoreFromWin(winPercent({ cp: -73 }))).toEqual({ cp: -73 });
  });

  it("pending game replays without analysis", async () => {
    // A re-queued game still holds its old moves and accuracies; none of them may show.
    const { gameId } = await seedFixtureGame(db, aliceAccount, blunder, {
      protocol: "review-pending",
      analysisStatus: "pending",
      withMoves: true,
      withEvals: true,
      analysisProtocol: "review-pending",
      whiteAccuracy: 81,
      blackAccuracy: 64,
    });
    const g = await review(alice, gameId);
    expect(g.analysisStatus).toBe("pending");
    expect(g.plies).toHaveLength(blunder.plies.length);
    expect(g.plies.map((p) => p.san)).toEqual(blunder.plies.map((p) => p.san));
    expect(g.plies.every((p) => p.analysis === null)).toBe(true);
    expect(g.analysed).toBe(false);
    expect(g.moments).toEqual([]);
    expect(g.accuracy).toEqual({ white: null, black: null });
    expect(g.startScore).toBeNull();
    expect(g.phases).toEqual({});
  });

  it("moves out of step with PGN are ignored", async () => {
    const { gameId } = await seedFixtureGame(db, aliceAccount, blunder, {
      protocol: "review-short-moves",
    });
    expect((await review(alice, gameId)).analysed).toBe(true);
    await db
      .delete(moves)
      .where(and(eq(moves.gameId, gameId), eq(moves.ply, blunder.plies.length)));
    const g = await review(alice, gameId);
    expect(g.analysed).toBe(false);
    expect(g.plies).toHaveLength(blunder.plies.length);
    expect(g.plies.every((p) => p.analysis === null)).toBe(true);
    expect(g.moments).toEqual([]);
  });

  it("moves whose uci disagrees with the PGN are ignored", async () => {
    const { gameId } = await seedFixtureGame(db, aliceAccount, blunder, {
      protocol: "review-bad-uci",
    });
    await db
      .update(moves)
      .set({ uci: "a1a1" })
      .where(and(eq(moves.gameId, gameId), eq(moves.ply, 10)));
    expect((await review(alice, gameId)).analysed).toBe(false);
  });

  it("game without clocks has hasClocks false", async () => {
    const { gameId } = await seedSyntheticGame(db, aliceAccount, {
      playedAt: "2026-10-02T12:00:00Z",
      timeClass: "blitz",
      result: "0-1",
      playerColor: "black",
      counts: { blunders: 2, mistakes: 1 },
      opponentCounts: { inaccuracies: 2 },
    });
    const g = await review(alice, gameId);
    expect(g.hasClocks).toBe(false);
    expect(g.plies.length).toBeGreaterThan(0);
    expect(g.plies.every((p) => p.clockAfter === null)).toBe(true);
    // Every drop is 0 here, so the tie-break orders the player's judged plies by ply.
    expect(g.moments).toEqual([2, 4, 6]);
  });

  it("the final checkmate scores mate 0", async () => {
    const chess = new Chess();
    const sans = ["f3", "e5", "g4", "Qh4#"];
    const plies = sans.map((san, i) => {
      const m = chess.move(san);
      const black = i % 2 === 1;
      return {
        ply: i + 1,
        color: black ? ("black" as const) : ("white" as const),
        san: m.san,
        uci: `${m.from}${m.to}`,
        fenAfter: m.after,
        winWhite: i === 3 ? 0 : 50 - i * 5,
        cpWhite: i === 3 ? null : -i * 40,
        mateWhite: i === 3 ? -1 : null,
        judgement: null,
        bestUci: null,
        clockSeconds: null,
        phase: "opening" as const,
      };
    });
    const mate: GameFixture = {
      ...blunder,
      id: "fools-mate",
      result: "0-1",
      playerColor: "black",
      plies,
    };
    const { gameId } = await seedFixtureGame(db, aliceAccount, mate, { protocol: "review-mate" });
    const g = await review(alice, gameId);
    expect(g.analysed).toBe(true);
    expect(g.plies[3]?.analysis?.score).toEqual({ mate: 0 });
    expect(g.plies[3]?.analysis?.winWhite).toBeCloseTo(0, 9);
    expect(g.plies[2]?.analysis?.score).toEqual({ cp: -80 });
  });

  it("a Lichess game links to its Site", async () => {
    const { gameId } = await seedFixtureGame(db, aliceAccount, blunder, {
      protocol: "review-lichess",
      platform: "lichess",
    });
    const g = await review(alice, gameId);
    expect(g.platform).toBe("lichess");
    expect(g.sourceUrl).toBe(blunder.source);
  });
});

describe("sourceUrl", () => {
  it("accepts the platform's own https links", () => {
    expect(sourceUrl("chesscom", { Link: "https://www.chess.com/game/live/123" })).toBe(
      "https://www.chess.com/game/live/123",
    );
    expect(sourceUrl("lichess", { Site: "https://lichess.org/HqjDTU4A" })).toBe(
      "https://lichess.org/HqjDTU4A",
    );
  });

  it("rejects anything else", () => {
    for (const bad of [
      "javascript:alert(1)",
      "https://evil.example/",
      "http://www.chess.com/game/live/123",
      "https://www.chess.com.evil.example/game/live/123",
      "https://user@evil.example/www.chess.com",
      "https://www.chess.com:8443/game/live/123",
      "not a url",
    ]) {
      expect(sourceUrl("chesscom", { Link: bad })).toBeNull();
    }
    expect(sourceUrl("chesscom", { Site: "https://www.chess.com/game/live/123" })).toBeNull();
    expect(sourceUrl("chesscom", {})).toBeNull();
    expect(sourceUrl("lichess", { Site: "javascript:alert(1)" })).toBeNull();
    expect(sourceUrl("lichess", { Site: "https://www.chess.com/game/live/123" })).toBeNull();
    expect(sourceUrl("lichess", { Link: "https://lichess.org/abc" })).toBeNull();
  });
});
