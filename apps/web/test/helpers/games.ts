import { randomUUID } from "node:crypto";
import { accountLinks, type Db, games, linkedAccounts, moves } from "@ci/db";
import { sql } from "drizzle-orm";
import type { AnalysisStatus, JudgementCounts, TimeClass } from "../../src/server/games/types";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const SAN = ["e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6"];
const UCI = ["e2e4", "e7e5", "g1f3", "b8c6", "f1b5", "a7a6", "b5a4", "g8f6"];

export async function resetGameTables(db: Db): Promise<void> {
  await db.execute(
    sql`truncate table linked_accounts, games, moves, position_evals, account_links cascade`,
  );
}

export async function seedAccount(
  db: Db,
  opts: { userId?: string; platform?: "chesscom"; username: string; verified?: boolean },
): Promise<{ accountId: string }> {
  const [row] = await db
    .insert(linkedAccounts)
    .values({
      platform: opts.platform ?? "chesscom",
      username: opts.username.toLowerCase(),
      displayName: opts.username,
    })
    .returning({ id: linkedAccounts.id });
  if (!row) throw new Error("seedAccount: no row");
  if (opts.userId) {
    await db.insert(accountLinks).values({
      userId: opts.userId,
      linkedAccountId: row.id,
      method: "username",
      verifiedAt: opts.verified ? new Date() : null,
    });
  }
  return { accountId: row.id };
}

export type SyntheticGame = {
  playedAt: string;
  timeClass: TimeClass;
  result: "1-0" | "0-1" | "1/2-1/2" | "*";
  playerColor?: "white" | "black";
  /** Player accuracy; null leaves both accuracies null. Default 70. */
  accuracy?: number | null;
  status?: AnalysisStatus;
  counts?: Partial<JudgementCounts>;
  opponentRating?: number;
  /** Write no moves rows (a game where the player made no analysed move). */
  noMoves?: boolean;
};

/** A short game whose player moves carry the given judgement counts. */
export async function seedSyntheticGame(
  db: Db,
  accountId: string,
  row: SyntheticGame,
): Promise<{ gameId: string }> {
  const colour = row.playerColor ?? "white";
  const status = row.status ?? "done";
  const accuracy = row.accuracy === undefined ? 70 : row.accuracy;
  const [g] = await db
    .insert(games)
    .values({
      accountId,
      platform: "chesscom",
      platformGameId: randomUUID(),
      pgn: `[Event "t"]\n\n1. ${SAN[0]} ${SAN[1]} 2. ${SAN[2]} ${SAN[3]} 1-0`,
      playedAt: new Date(row.playedAt),
      timeClass: row.timeClass,
      rated: true,
      playerColor: colour,
      whiteName: colour === "white" ? "me" : "opp",
      blackName: colour === "white" ? "opp" : "me",
      whiteRating: colour === "white" ? 1500 : (row.opponentRating ?? 1500),
      blackRating: colour === "black" ? 1500 : (row.opponentRating ?? 1500),
      result: row.result,
      analysisStatus: status,
      analysisProtocol: status === "done" ? "test-1" : null,
      whiteAccuracy: status === "done" && colour === "white" ? accuracy : null,
      blackAccuracy: status === "done" && colour === "black" ? accuracy : null,
    })
    .returning({ id: games.id });
  if (!g) throw new Error("seedSyntheticGame: no row");

  if (status === "done" && !row.noMoves) {
    const wanted: ("blunder" | "mistake" | "inaccuracy")[] = [
      ...Array(row.counts?.blunders ?? 0).fill("blunder"),
      ...Array(row.counts?.mistakes ?? 0).fill("mistake"),
      ...Array(row.counts?.inaccuracies ?? 0).fill("inaccuracy"),
    ];
    const playerMoves = Math.max(4, wanted.length);
    const rows: (typeof moves.$inferInsert)[] = [];
    for (let i = 0; i < playerMoves * 2; i++) {
      const mover = i % 2 === 0 ? "white" : "black";
      const mine = mover === colour;
      const k = Math.floor(i / 2);
      rows.push({
        gameId: g.id,
        ply: i + 1,
        color: mover,
        san: SAN[i % SAN.length] as string,
        uci: UCI[i % UCI.length] as string,
        fenBefore: START_FEN,
        bestUci: null,
        winBefore: 50,
        winAfter: 50,
        deltaW: 0,
        accuracy: 80,
        judgement: mine ? (wanted[k] ?? null) : null,
        phase: "middlegame",
        decided: false,
      });
    }
    await db.insert(moves).values(rows);
  }
  return { gameId: g.id };
}
