import { assertSafeTestDatabase, createDb, user } from "@ci/db";
import { eq } from "drizzle-orm";
import { seedAccount, seedSyntheticGame } from "../../test/helpers/games";

// Run by review-reveal.spec.ts (through tsx). Links the signed-in e2e user to a synthetic account
// and seeds one analysed synthetic game (never real data) whose player moves carry a blunder, a
// mistake and an inaccuracy, so the analysis reveal has three beats. Prints `GAME=<id>`.

const email = process.argv[2];
if (!email) throw new Error("usage: seed-review.ts <email>");
const db = createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL));
const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
if (!u) throw new Error(`no e2e user ${email}`);

const { accountId } = await seedAccount(db, { userId: u.id, username: "e2e-SyntheticReviewer" });
const { gameId } = await seedSyntheticGame(db, accountId, {
  playedAt: "2026-10-10T10:00:00.000Z",
  timeClass: "rapid",
  result: "0-1",
  playerColor: "white",
  counts: { blunders: 1, mistakes: 1, inaccuracies: 1 },
  playerMoves: 12,
});
console.log(`GAME=${gameId}`);
process.exit(0);
