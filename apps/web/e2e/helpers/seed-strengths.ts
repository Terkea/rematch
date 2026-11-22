import { pathToFileURL } from "node:url";
import { assertSafeTestDatabase, createDb, type Db, linkedAccounts } from "@ci/db";
import { gameMetrics, games, moveFeatures, weaknessStateHistory } from "@ci/db/generated";
import { and, eq, inArray, sql } from "drizzle-orm";
import { seedE2eCoach } from "./seed-coach";
import { E2E_ACCOUNT } from "./seed-games";

// The strengths e2e seed: the coach world (seed-coach.ts), plus what the strengths page reads
// that the coach seed leaves out. The two public fixture games the player had as Black
// (`e2e-analysed`, 2026-10-31, and the short attack, 2026-10-29) get blitz `game_metrics` rows
// carrying strength plies on real moves of those games, with `move_features` saying the position
// was critical and Maia's difficulty is pending; W8.basics' improving transition gets its stored
// p_change. Synthetic rows only. `clear` takes the additions away (the coach world stays), so
// every later spec sees the world the coach seed writes.

const DEFAULT_URL = "postgres://postgres:postgres@localhost:55432/chess_e2e_test";
const PROTOCOL = "e2e-strengths";
const idOf = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;

/**
 * Black's moves in the fixtures, with their gaps: three quiet moves in a balanced game (the
 * short attack's 8… Qb6, and 27… Rf8 and 14… Na6 in `e2e-analysed`), and the short attack's
 * 10… Qxf4, a free capture right after White's blunder, which the page must leave out (N11)
 * although its gap is the biggest.
 */
const SPOTS = [
  { gameId: idOf(3), ply: 16, gap: 31, kind: "tactic_found" },
  { gameId: idOf(1), ply: 54, gap: 24, kind: "only_move_found" },
  { gameId: idOf(1), ply: 28, gap: 18, kind: "tactic_found" },
  { gameId: idOf(3), ply: 20, gap: 40, kind: "tactic_found" },
] as const;

function dbOf(db?: Db): Db {
  return db ?? createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL ?? DEFAULT_URL));
}

async function accountOf(db: Db): Promise<string> {
  const [a] = await db
    .select({ id: linkedAccounts.id })
    .from(linkedAccounts)
    .where(eq(linkedAccounts.username, E2E_ACCOUNT));
  if (!a) throw new Error("seed the coach first");
  return a.id;
}

/** Removes this seed's additions (the coach seed's own rows stay). */
export async function clearE2eStrengths(opts: { db?: Db } = {}): Promise<void> {
  const db = dbOf(opts.db);
  const accountId = await accountOf(db);
  const ids = [...new Set(SPOTS.map((s) => s.gameId))];
  await db.delete(moveFeatures).where(eq(moveFeatures.enrichmentProtocol, PROTOCOL));
  await db
    .delete(gameMetrics)
    .where(and(eq(gameMetrics.accountId, accountId), inArray(gameMetrics.gameId, ids)));
  await db.execute(sql`update weakness_state_history set p_change = null
    where account_id = ${accountId} and series_key = 'W8.basics' and to_state = 'improving'`);
}

/** Seeds the coach world, then the strengths additions (see the module comment). */
export async function seedE2eStrengths(opts: { db?: Db } = {}): Promise<void> {
  const db = dbOf(opts.db);
  await seedE2eCoach({ db });
  const accountId = await accountOf(db);
  await clearE2eStrengths({ db });
  const ids = [...new Set(SPOTS.map((s) => s.gameId))];
  const rows = await db
    .select({ id: games.id, playedAt: games.playedAt, timeClass: games.timeClass })
    .from(games)
    .where(inArray(games.id, ids));
  for (const g of rows) {
    await db.insert(gameMetrics).values({
      gameId: g.id,
      accountId,
      timeClass: g.timeClass,
      playedAt: g.playedAt,
      protocol: PROTOCOL,
      metrics: {
        version: 1,
        strengths: SPOTS.filter((s) => s.gameId === g.id).map((s) => ({
          kind: s.kind,
          ply: s.ply,
        })),
      },
    });
  }
  await db.insert(moveFeatures).values(
    SPOTS.map((s) => ({
      gameId: s.gameId,
      ply: s.ply,
      enrichmentProtocol: PROTOCOL,
      difficultyStatus: "pending_maia",
      isCritical: true,
      multipvGap: s.gap,
    })),
  );
  await db
    .update(weaknessStateHistory)
    .set({ pChange: 0.85 })
    .where(
      and(
        eq(weaknessStateHistory.accountId, accountId),
        eq(weaknessStateHistory.seriesKey, "W8.basics"),
        eq(weaknessStateHistory.toState, "improving"),
      ),
    );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv[2] === "clear") await clearE2eStrengths();
  else await seedE2eStrengths();
  console.log("STRENGTHS=ok");
  process.exit(0);
}
