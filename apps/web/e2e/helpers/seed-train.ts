import { pathToFileURL } from "node:url";
import { assertSafeTestDatabase, createDb, type Db, linkedAccounts, user } from "@ci/db";
import { drillItems } from "@ci/db/generated";
import { Chess } from "chess.js";
import { eq, sql } from "drizzle-orm";
import { resolveCoachScope } from "../../src/server/profile";
import { refreshItems } from "../../src/server/train/store";
import { E2E_USER, isE2eUsername, REFUSAL } from "./seed-games";

// The trainer e2e seed (ADR 0090), run after the games and coach seeds: it clears the e2e user's
// drill items, makes them again from the seeded analysis, and takes them out of the holdout so the
// session is the same on every run (the holdout itself is unit tested). It prints `TRAIN=<json>`:
// each item's id, a move that passes and a legal move that does not. `due` prints each item's
// `due_on` instead. Synthetic e2e data only; it refuses a database with a real account.

const DEFAULT_URL = "postgres://postgres:postgres@localhost:55432/chess_e2e_test";

export type E2eTrainItem = { id: string; right: string; wrong: string };

function dbOf(): Db {
  return createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL ?? DEFAULT_URL));
}

async function userId(db: Db, email: string): Promise<string> {
  const accounts = await db.select({ username: linkedAccounts.username }).from(linkedAccounts);
  if (accounts.some((a) => !isE2eUsername(a.username))) throw new Error(REFUSAL);
  const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
  if (!u) throw new Error(`no user ${email}; seed the games first`);
  return u.id;
}

export async function seedE2eTrain(email = E2E_USER): Promise<E2eTrainItem[]> {
  const db = dbOf();
  const uid = await userId(db, email);
  await db.delete(drillItems).where(eq(drillItems.userId, uid));
  // The refresh mark too, so the refresh runs on the same games again (ADR 0090 Amendment 1).
  await db.execute(sql`delete from drill_refreshes where user_id = ${uid}`);
  const scope = await resolveCoachScope(db, uid, {});
  if (!scope) throw new Error("no coach scope; seed the coach first");
  await refreshItems(db, uid, scope);
  await db.update(drillItems).set({ heldOut: false }).where(eq(drillItems.userId, uid));
  const items = await db
    .select({ id: drillItems.id, fen: drillItems.fen, accepted: drillItems.acceptedUci })
    .from(drillItems)
    .where(eq(drillItems.userId, uid));
  return items.map((i) => {
    const legal = new Chess(i.fen)
      .moves({ verbose: true })
      .map((m) => `${m.from}${m.to}${m.promotion ?? ""}`);
    const wrong = legal.find((u) => !i.accepted.includes(u) && u.length === 4) ?? "";
    return { id: i.id, right: i.accepted[0] ?? "", wrong };
  });
}

export async function dueDays(email = E2E_USER): Promise<Record<string, string | null>> {
  const db = dbOf();
  const uid = await userId(db, email);
  const rows = (await db.execute(
    sql`select id, due_on::text as due_on from drill_items where user_id = ${uid}`,
  )) as unknown as { id: string; due_on: string | null }[];
  return Object.fromEntries(rows.map((r) => [r.id, r.due_on]));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [command] = process.argv.slice(2);
  if (command === "due") console.log(`DUE=${JSON.stringify(await dueDays())}`);
  else console.log(`TRAIN=${JSON.stringify(await seedE2eTrain())}`);
  process.exit(0);
}
