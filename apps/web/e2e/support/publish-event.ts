import { assertSafeTestDatabase, createDb, insertUserEvent, linkedAccounts, user } from "@ci/db";
import { eq } from "drizzle-orm";
import { seedAccount } from "../../test/helpers/games";

// Run by realtime.spec.ts (through tsx with `--conditions development`, like seed-games.ts).
//   publish-event.ts <email> <imported> <total>
// Links the e2e user to the synthetic Lichess account `e2e-realtime` (once), then stores and
// announces `import.progress { imported, total }` for it through `insertUserEvent`, the same
// insert plus NOTIFY the worker uses (ADR 0034). The seed helpers refuse anything but a `*_test`
// database that holds no real accounts.

const [email, imported, total] = process.argv.slice(2);
if (!email || !imported || !total) throw new Error("usage: publish-event.ts <email> <n> <total>");
const db = createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL));
const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
if (!u) throw new Error(`no e2e user ${email}; sign in first`);

const USERNAME = "e2e-realtime";
const [have] = await db
  .select({ id: linkedAccounts.id })
  .from(linkedAccounts)
  .where(eq(linkedAccounts.username, USERNAME));
const accountId =
  have?.id ??
  (await seedAccount(db, { userId: u.id, platform: "lichess", username: USERNAME, verified: true }))
    .accountId;

const id = await insertUserEvent(db, {
  accountId,
  type: "import.progress",
  payload: { imported: Number(imported), total: Number(total) },
});
console.log(`EVENT=${id}`);
process.exit(0);
