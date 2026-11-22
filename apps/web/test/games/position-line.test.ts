import { moveFeatures, moves } from "@ci/db/generated";
import { games as fixtures } from "@ci/ui/fixtures";
import { Chess } from "chess.js";
import { and, eq } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { LINE_CONTEXT, positionLine } from "../../src/server/games/position-line";
import { resetGameTables, seedAccount, seedFixtureGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// The position explorer's expanded data (feat/position-explorer): the game around a cited ply
// and the engine's stored line from the position before it, for the user's own games only.

const db = webTestDb();
const game = fixtures.middlegameBlunder;
const CITED = 55; // 28. Nd2??
const fenBefore = (ply: number) =>
  ply === 1 ? game.startFen : (game.plies[ply - 2]?.fenAfter ?? "");

let alice: string;
let bob: string;
let gameId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  alice = (await createTestSession(db)).user.id;
  bob = (await createTestSession(db)).user.id;
  const { accountId } = await seedAccount(db, { userId: alice, username: "alice" });
  ({ gameId } = await seedFixtureGame(db, accountId, game));
});

/** A legal line from the position before the cited move, as UCI. */
function legalLine(n: number): string[] {
  const chess = new Chess(fenBefore(CITED));
  const out: string[] = [];
  for (let i = 0; i < n; i++) {
    const m = chess.moves({ verbose: true })[0];
    if (!m) break;
    chess.move(m);
    out.push(`${m.from}${m.to}${m.promotion ?? ""}`);
  }
  return out;
}

describe("positionLine", () => {
  it("gives the game around the ply, from the PGN", async () => {
    const line = await positionLine(db, alice, gameId, CITED);
    expect(line?.game.map((m) => m.ply)).toEqual(
      Array.from({ length: 2 * LINE_CONTEXT + 1 }, (_, i) => CITED - LINE_CONTEXT + i),
    );
    const cited = line?.game.find((m) => m.ply === CITED);
    expect(cited).toMatchObject({ san: "Nd2", uci: "e4d2", color: "white" });
    expect(cited?.fenBefore).toBe(fenBefore(CITED));
  });

  it("clamps the window at the start of the game", async () => {
    const line = await positionLine(db, alice, gameId, 2);
    expect(line?.game[0]?.ply).toBe(1);
    expect(line?.game[0]?.fenBefore).toBe(game.startFen);
  });

  it("the engine line is the stored best line, cut at the first illegal move", async () => {
    const pv = legalLine(4);
    await db.insert(moveFeatures).values({
      gameId,
      ply: CITED,
      enrichmentProtocol: "test-1",
      bestPv: [...pv, "a1a1"],
    });
    const line = await positionLine(db, alice, gameId, CITED);
    expect(line?.engine.map((m) => m.uci)).toEqual(pv);
    expect(line?.engine.map((m) => m.ply)).toEqual([55, 56, 57, 58]);
    expect(line?.engine[0]?.fenBefore).toBe(fenBefore(CITED));
  });

  it("without a stored line, the engine's best move alone", async () => {
    const [best] = legalLine(1);
    await db
      .update(moves)
      .set({ bestUci: best ?? null })
      .where(and(eq(moves.gameId, gameId), eq(moves.ply, CITED)));
    const line = await positionLine(db, alice, gameId, CITED);
    expect(line?.engine.map((m) => m.uci)).toEqual([best]);
  });

  it("no engine move gives an empty engine line, never an invented one", async () => {
    const line = await positionLine(db, alice, gameId, CITED);
    expect(line?.engine).toEqual([]);
  });

  it("another user's game, a bad id or a ply outside the game gives null", async () => {
    expect(await positionLine(db, bob, gameId, CITED)).toBeNull();
    expect(await positionLine(db, alice, "not-a-uuid", CITED)).toBeNull();
    expect(await positionLine(db, alice, gameId, 0)).toBeNull();
    expect(await positionLine(db, alice, gameId, game.plies.length + 1)).toBeNull();
  });
});
