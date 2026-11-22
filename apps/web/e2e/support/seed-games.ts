import { assertSafeTestDatabase, createDb, user } from "@ci/db";
import { eq } from "drizzle-orm";
import { seedAccount, seedSyntheticGame } from "../../test/helpers/games";

// Run by games.spec.ts (through tsx, like reset-db.ts). Links the signed-in e2e user to a
// synthetic chess.com account and seeds 40 synthetic games (never real data) across two months,
// three time controls, both colours, every result and a pending one, so the explorer has a
// grouped first page, a second page and long names to wrap. The seed helpers refuse anything but
// a `*_test` database.

const email = process.argv[2];
if (!email) throw new Error("usage: seed-games.ts <email>");
const db = createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL));
const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
if (!u) throw new Error(`no e2e user ${email}`);

const { accountId } = await seedAccount(db, {
  userId: u.id,
  username: "e2e-SyntheticPlayerWithAVeryLongName",
  verified: true,
});
const classes = ["rapid", "blitz", "bullet"] as const;
const results = ["1-0", "0-1", "1/2-1/2", "0-1"] as const;
for (let i = 0; i < 40; i++) {
  await seedSyntheticGame(db, accountId, {
    playedAt: new Date(Date.UTC(2026, 9, 1 + Math.floor(i / 3), 9 + (i % 3))).toISOString(),
    timeClass: classes[i % 3] ?? "rapid",
    result: results[i % 4] ?? "1-0",
    playerColor: i % 2 === 0 ? "white" : "black",
    accuracy: 40 + ((i * 13) % 55),
    status: i === 39 ? "pending" : "done",
    counts: { blunders: i % 5, mistakes: (i * 3) % 4, inaccuracies: i % 3 },
    opponentRating: 900 + ((i * 37) % 900),
  });
}
console.log(`seeded 40 games for ${email}`);
process.exit(0);
