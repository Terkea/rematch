import { pathToFileURL } from "node:url";
import {
  assertSafeTestDatabase,
  createDb,
  type Db,
  games,
  linkedAccounts,
  moves,
  user,
} from "@ci/db";
import { games as fixtures } from "@ci/ui/fixtures";
import { Chess } from "chess.js";
import { and, eq } from "drizzle-orm";
import { seedAccount, seedFixtureGame, seedSyntheticGame } from "../../test/helpers/games";

// The games e2e seed (M5a Task 13): synthetic and public fixture data only, never a real
// player's games. It runs against the throwaway e2e stack's own database (E2E_DATABASE_URL, which
// must be a `*_test` one). Before writing anything it refuses a database whose linked accounts
// are not all e2e ones (`e2e-…`, or the accounts e2e's `fake…`): if the default compose project,
// which holds the owner's games, is up by mistake, nothing is written. Run as a script (through
// tsx with `--conditions development`, like reset-db.ts) it prints `SEED=<json>`.

export const E2E_USER = "adult@example.invalid";
export const E2E_ACCOUNT = "e2e-fixtures";
const DEFAULT_URL = "postgres://postgres:postgres@localhost:55432/chess_e2e_test";
const REFUSAL = "Refusing to seed: this database holds real accounts";

export type E2eGames = { analysedId: string; pendingId: string; longId: string };

/** True for the e2e suite's own accounts. */
export function isE2eUsername(name: string): boolean {
  return name.startsWith("e2e-") || name.startsWith("fake");
}

const KEYS = { analysed: "e2e-analysed", pending: "e2e-pending", long: "e2e-long" } as const;

async function existing(db: Db, accountId: string): Promise<E2eGames | null> {
  const id = async (key: string) =>
    (
      await db
        .select({ id: games.id })
        .from(games)
        .where(and(eq(games.accountId, accountId), eq(games.platformGameId, key)))
    )[0]?.id;
  const [analysedId, pendingId, longId] = await Promise.all([
    id(KEYS.analysed),
    id(KEYS.pending),
    id(KEYS.long),
  ]);
  return analysedId && pendingId && longId ? { analysedId, pendingId, longId } : null;
}

/** Fixed game ids: the overview's bootstrap bands are seeded by game id (visual baselines). */
const idOf = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;

const RESULTS = ["1-0", "0-1", "1/2-1/2", "0-1"] as const;
const DAY = 86_400_000;

/**
 * Links the e2e user (default the adult one) to `e2e-fixtures` with: the three public fixture
 * games (one of them, the newest loss, is `analysedId`), 30 synthetic rapid and blitz games dated
 * 2026-09-01 to 2026-10-28, one pending game and one 300-ply game. A second run returns the same
 * games and writes nothing.
 */
export async function seedE2eGames(opts: { db?: Db; email?: string } = {}): Promise<E2eGames> {
  const db =
    opts.db ?? createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL ?? DEFAULT_URL));
  const accounts = await db.select({ username: linkedAccounts.username }).from(linkedAccounts);
  if (accounts.some((a) => !isE2eUsername(a.username))) throw new Error(REFUSAL);

  const email = opts.email ?? E2E_USER;
  const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
  if (!u) throw new Error(`no e2e user ${email}; sign in first`);

  const [have] = await db
    .select({ id: linkedAccounts.id })
    .from(linkedAccounts)
    .where(eq(linkedAccounts.username, E2E_ACCOUNT));
  if (have) {
    const ids = await existing(db, have.id);
    if (!ids) throw new Error(`${E2E_ACCOUNT} exists but its games are incomplete`);
    return ids;
  }

  const { accountId } = await seedAccount(db, {
    userId: u.id,
    username: E2E_ACCOUNT,
    verified: true,
  });

  // The fixtures, newest first. The middlegame one is seen from Black, so it is a loss.
  const { gameId: analysedId } = await seedFixtureGame(db, accountId, fixtures.middlegameBlunder, {
    id: idOf(1),
    playerColor: "black",
    playedAt: new Date("2026-10-31T18:00:00Z"),
    platformGameId: KEYS.analysed,
  });
  await seedFixtureGame(db, accountId, fixtures.endgameGrind, {
    id: idOf(2),
    playedAt: new Date("2026-10-30T18:00:00Z"),
  });
  await seedFixtureGame(db, accountId, fixtures.shortAttack, {
    id: idOf(3),
    playedAt: new Date("2026-10-29T18:00:00Z"),
  });

  // The fixture carries an engine move on its first plies only. Give ply 4 one, a legal move
  // other than the one played, so "Show the engine's next move" has an arrow at ply 3.
  const [fourth] = await db
    .select({ fen: moves.fenBefore, uci: moves.uci })
    .from(moves)
    .where(and(eq(moves.gameId, analysedId), eq(moves.ply, 4)));
  if (!fourth) throw new Error("the analysed fixture has no ply 4");
  const other = new Chess(fourth.fen)
    .moves({ verbose: true })
    .map((m) => `${m.from}${m.to}${m.promotion ?? ""}`)
    .find((uci) => uci !== fourth.uci);
  await db
    .update(moves)
    .set({ bestUci: other ?? null })
    .where(and(eq(moves.gameId, analysedId), eq(moves.ply, 4)));

  for (let i = 0; i < 30; i++) {
    await seedSyntheticGame(db, accountId, {
      id: idOf(10 + i),
      playedAt: new Date(Date.UTC(2026, 8, 1, 10 + (i % 3)) + i * 2 * DAY).toISOString(),
      timeClass: i % 2 === 0 ? "rapid" : "blitz",
      result: RESULTS[i % 4] ?? "1-0",
      playerColor: i % 3 === 0 ? "black" : "white",
      accuracy: 45 + ((i * 17) % 45),
      counts: { blunders: i % 4, mistakes: (i * 3) % 4, inaccuracies: i % 3 },
      opponentRating: 1200 + ((i * 41) % 600),
    });
  }

  const { gameId: pendingId } = await seedSyntheticGame(db, accountId, {
    id: idOf(4),
    playedAt: "2026-10-15T09:30:00.000Z",
    timeClass: "rapid",
    result: "1-0",
    status: "pending",
  });
  const { gameId: longId } = await seedSyntheticGame(db, accountId, {
    id: idOf(5),
    playedAt: "2026-10-20T09:30:00.000Z",
    timeClass: "rapid",
    result: "1/2-1/2",
    playerMoves: 150,
    counts: { blunders: 2, mistakes: 3, inaccuracies: 4 },
  });
  for (const [id, key] of [
    [pendingId, KEYS.pending],
    [longId, KEYS.long],
  ] as const) {
    await db.update(games).set({ platformGameId: key }).where(eq(games.id, id));
  }
  return { analysedId, pendingId, longId };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const ids = await seedE2eGames(process.argv[2] ? { email: process.argv[2] } : {});
  console.log(`SEED=${JSON.stringify(ids)}`);
  process.exit(0);
}
