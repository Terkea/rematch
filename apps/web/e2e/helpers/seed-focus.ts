import { pathToFileURL } from "node:url";
import { assertSafeTestDatabase, createDb, type Db, linkedAccounts, user } from "@ci/db";
import { focuses } from "@ci/db/generated";
import { eq, sql } from "drizzle-orm";
import { resolveCoachScope } from "../../src/server/profile";
import { seedProfile } from "../../test/helpers/profile";
import { E2E_USER, isE2eUsername, REFUSAL } from "./seed-games";

// The weekly focus e2e seed (ADR 0091), run after the games and coach seeds: it replaces the e2e
// user's rapid focuses with one running focus over 5 games, started before every seeded game, so
// the card reads as closed. The coach seed gives every rapid game 8 W3.shallow chances (7 events
// in the first 5) and no W4.missed point. `met` targets 50% on W3.shallow, `not_met` 10%, and
// `inconclusive` is W4.missed (no chance). `clear` removes the user's focuses; `zero` gives W2
// an estimate of 0% (a rate that cannot go down: no target), which every other state removes. Synthetic data only.

const DEFAULT_URL = "postgres://postgres:postgres@localhost:55432/chess_e2e_test";

const CLOSED = {
  met: { seriesKey: "W3.shallow", targetRate: 0.5 },
  not_met: { seriesKey: "W3.shallow", targetRate: 0.1 },
  inconclusive: { seriesKey: "W4.missed", targetRate: 0.05 },
} as const;

export type ClosedKind = keyof typeof CLOSED;

async function userId(db: Db, email: string): Promise<string> {
  const accounts = await db.select({ username: linkedAccounts.username }).from(linkedAccounts);
  if (accounts.some((a) => !isE2eUsername(a.username))) throw new Error(REFUSAL);
  const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
  if (!u) throw new Error(`no user ${email}; seed the games first`);
  return u.id;
}

export async function seedE2eFocus(
  state: ClosedKind | "clear" | "zero",
  email = E2E_USER,
): Promise<void> {
  const db = createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL ?? DEFAULT_URL));
  const uid = await userId(db, email);
  await db.delete(focuses).where(eq(focuses.userId, uid));
  const scope = await resolveCoachScope(db, uid, { tc: "rapid" });
  if (!scope) throw new Error("no rapid coach scope; seed the coach first");
  // The coach seed has no W2: only `zero` adds one, and every other state removes it.
  await db.execute(sql`
    delete from weakness_estimates
    where account_id = ${scope.accountId} and time_class = 'rapid' and series_key = 'W2'`);
  if (state === "clear") return;
  if (state === "zero") {
    await seedProfile(db, scope.accountId, {
      timeClass: "rapid",
      estimates: [
        {
          seriesKey: "W2",
          mean: 0,
          k: 0,
          kUnweighted: 0,
          ciLow: 0,
          ciHigh: 0.05,
          state: "not_an_issue",
          priority: 0,
        },
      ],
    });
    return;
  }
  const c = CLOSED[state];
  await db.insert(focuses).values({
    userId: uid,
    accountId: scope.accountId,
    timeClass: "rapid",
    seriesKey: c.seriesKey,
    side: "both",
    targetRate: c.targetRate,
    gamesN: 5,
    startedAt: new Date("2026-08-01T00:00:00Z"),
    baselineRate: 0.2,
    baselineK: 24,
    baselineN: 120,
    peerRate: null,
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [state] = process.argv.slice(2);
  if (state !== "clear" && state !== "zero" && !(state && state in CLOSED)) {
    throw new Error(`unknown state ${state}`);
  }
  await seedE2eFocus(state as ClosedKind | "clear" | "zero");
  console.log("FOCUS=ok");
  process.exit(0);
}
