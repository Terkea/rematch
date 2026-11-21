import { randomUUID } from "node:crypto";
import { type Db, linkedAccounts } from "@ci/db";
import type { NormalizedGame } from "../../src/importers/types.js";

export const SCHOLARS = "1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0";

export async function insertAccount(
  db: Db,
  username = "hero",
  platform: "lichess" | "chesscom" = "lichess",
) {
  const [row] = await db
    .insert(linkedAccounts)
    .values({ platform, username: username.toLowerCase(), displayName: username })
    .returning();
  if (!row) throw new Error("insert failed");
  return row;
}

export function makeGame(over: Partial<NormalizedGame> = {}): NormalizedGame {
  return {
    platform: "lichess",
    platformGameId: randomUUID().slice(0, 8),
    pgn: SCHOLARS,
    playedAt: new Date("2026-01-01T00:00:00Z"),
    timeClass: "blitz",
    baseSeconds: 300,
    incrementSeconds: 0,
    rated: true,
    white: { name: "Hero", rating: 1500 },
    black: { name: "Villain", rating: 1500 },
    result: "1-0",
    termination: null,
    eco: null,
    openingName: null,
    lichessEvals: null,
    ...over,
  };
}
